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

    // todo we can emit the event twice if the contract started to produce logs. Do something with it
    // todo here should be check

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
        rainbow_bridge_events,
        tkn_near_events,
        wentokensir_events,
        wrap_near_events,
    ) = try_join!(
        nep141_future,
        rainbow_bridge_future,
        tkn_near_future,
        wentokensir_future,
        wrap_near_future
    )?;

    events.extend(nep141_events);
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
        &block_header.prev_hash,
        base.contract_account_id.clone(),
        &custom.affected_id,
        &custom.delta,
    )
    .await?;

    // todo discuss do we want to leave this code here forever
    // I calc it just to check the logic. Should be dropped in the end
    // actually they could be different if the block contains more than one operations with this account
    // but it's enough to check at least something
    let correct_value = balance_utils::get_balance_from_rpc(
        json_rpc_client,
        &block_header.hash,
        base.contract_account_id.clone(),
        custom.affected_id.as_str(),
    )
    .await?;
    if correct_value != absolute_amount.to_string().parse::<u128>()? {
        tracing::error!(
            target: crate::INDEXER,
            "{} different amounts for {}: expected {}, actual {}. Index {}",
            block_header.height,
            custom.affected_id.as_str(),
            correct_value,
            absolute_amount.to_string().parse::<u128>()?,
            base.event_index,
        );
    }

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
