use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use futures::future::try_join_all;
use futures::try_join;
use near_lake_framework::near_indexer_primitives;

pub(crate) mod balance_utils;
mod legacy;
mod nep141_events;

struct FtEvent {
    pub affected_id: near_primitives::types::AccountId,
    pub involved_id: Option<near_primitives::types::AccountId>,
    pub delta: BigDecimal,
    pub cause: String,
    pub memo: Option<String>,
}

pub(crate) async fn store_ft(
    pool: &sqlx::Pool<sqlx::Postgres>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    ft_balance_cache: &crate::FtBalanceCache,
) -> anyhow::Result<()> {
    let mut events: Vec<CoinEvent> = vec![];

    let events_futures = streamer_message.shards.iter().map(|shard| {
        collect_ft_for_shard(json_rpc_client, streamer_message, ft_balance_cache, shard)
    });
    for events_by_shard in try_join_all(events_futures).await? {
        events.extend(events_by_shard);
    }

    // After all the parallelism, events should be still sorted accurately,
    // So that we could go backwards and check the balance for each affected user
    let mut accounts_with_changes = std::collections::HashSet::new();
    for event in events.iter().rev() {
        if accounts_with_changes.insert(&event.affected_account_id) {
            balance_utils::check_balance(
                json_rpc_client,
                &streamer_message.block.header,
                &event.contract_account_id,
                &event.affected_account_id,
                &event.absolute_amount,
            )
            .await
            .map_err(|x| {
                events
                    .iter()
                    .filter(|e| e.affected_account_id == event.affected_account_id)
                    .for_each(|e| {
                        tracing::error!(target: crate::INDEXER, "{:?}", e);
                    });
                x
            })?;
        }
    }

    crate::models::chunked_insert(pool, &events).await
}

async fn collect_ft_for_shard(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    ft_balance_cache: &crate::FtBalanceCache,
    shard: &near_indexer_primitives::IndexerShard,
) -> anyhow::Result<Vec<CoinEvent>> {
    let mut events: Vec<CoinEvent> = vec![];
    let nep141_future = nep141_events::collect_nep141_events(
        json_rpc_client,
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        ft_balance_cache,
    );
    let aurora_future = legacy::collect_aurora(
        json_rpc_client,
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        ft_balance_cache,
    );
    let rainbow_bridge_future = legacy::collect_rainbow_bridge(
        json_rpc_client,
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        ft_balance_cache,
    );
    let tkn_near_future = legacy::collect_tkn_near(
        json_rpc_client,
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        ft_balance_cache,
    );
    let wentokensir_future = legacy::collect_wentokensir(
        json_rpc_client,
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        ft_balance_cache,
    );
    let wrap_near_future = legacy::collect_wrap_near(
        json_rpc_client,
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        ft_balance_cache,
    );

    let (
        nep141_events,
        aurora_events,
        rainbow_bridge_events,
        tkn_near_events,
        wentokensir_events,
        wrap_near_events,
    ) = try_join!(
        nep141_future,
        aurora_future,
        rainbow_bridge_future,
        tkn_near_future,
        wentokensir_future,
        wrap_near_future
    )?;

    events.extend(nep141_events);
    events.extend(aurora_events);
    events.extend(rainbow_bridge_events);
    events.extend(tkn_near_events);
    events.extend(wentokensir_events);
    events.extend(wrap_near_events);
    Ok(events)
}

async fn build_event(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    ft_balance_cache: &crate::FtBalanceCache,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    base: crate::db_adapters::EventBase,
    custom: FtEvent,
) -> anyhow::Result<CoinEvent> {
    let absolute_amount = balance_utils::update_cache_and_get_balance(
        json_rpc_client,
        ft_balance_cache,
        &base.status,
        block_header,
        base.contract_account_id.clone(),
        &custom.affected_id,
        &custom.delta,
    )
    .await?;

    Ok(CoinEvent {
        event_index: base.event_index,
        receipt_id: base.receipt_id,
        block_timestamp: base.block_timestamp,
        contract_account_id: base.contract_account_id.to_string(),
        affected_account_id: custom.affected_id.to_string(),
        involved_account_id: custom.involved_id.map(|id| id.to_string()),
        delta_amount: custom.delta,
        absolute_amount,
        // standard: "".to_string(),
        // coin_id: "".to_string(),
        cause: custom.cause,
        status: crate::db_adapters::get_status(&base.status),
        event_memo: custom.memo,
    })
}
