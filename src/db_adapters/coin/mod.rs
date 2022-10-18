use crate::db_adapters::contracts;
use crate::models;
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use futures::future::try_join_all;
use futures::try_join;
use near_lake_framework::near_indexer_primitives;
use near_primitives::types::AccountId;
use std::str::FromStr;

pub(crate) mod balance_utils;
mod legacy;
mod nep141_events;

pub const FT: &str = "FT_NEP141";
pub const FT_LEGACY: &str = "FT_LEGACY";

struct FtEvent {
    pub affected_id: AccountId,
    pub involved_id: Option<AccountId>,
    pub delta: BigDecimal,
    pub cause: String,
    pub memo: Option<String>,
}

pub(crate) async fn store_ft(
    pool: &sqlx::Pool<sqlx::Postgres>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    ft_balance_cache: &crate::FtBalanceCache,
    contracts: &crate::ActiveContracts,
) -> anyhow::Result<()> {
    let mut events: Vec<CoinEvent> = vec![];

    let events_futures = streamer_message.shards.iter().map(|shard| {
        collect_ft_for_shard(
            json_rpc_client,
            streamer_message,
            ft_balance_cache,
            contracts,
            shard,
        )
    });
    for events_by_shard in try_join_all(events_futures).await? {
        events.extend(events_by_shard);
    }

    // We go by all collected events (sorted historically) and check the latest balance for each affected user
    // (we go backwards and check only first entry for each user)
    let mut accounts_with_changes = std::collections::HashSet::new();
    // We also collect all the contracts with the inconsistency
    let mut contracts_with_inconsistency = std::collections::HashSet::new();
    for event in events.iter().rev() {
        if accounts_with_changes.insert(event.affected_account_id.clone()) {
            balance_utils::check_balance(
                json_rpc_client,
                &streamer_message.block.header,
                &event.contract_account_id,
                &event.affected_account_id,
                &event.absolute_amount,
            )
            .await
            .map_err(|x| {
                contracts_with_inconsistency.insert(event.contract_account_id.clone());
                events
                    .iter()
                    .filter(|e| e.affected_account_id == event.affected_account_id)
                    .for_each(|e| {
                        tracing::error!(target: crate::LOGGING_PREFIX, "{:?}", e);
                    });
                x
            })?;
        }
    }
    // Dropping all the data with inconsistency.
    // It may give us gaps in enumeration, e.g. nep141 events will have numbers 0, 1, 3, 7, 8
    // It does not break the sorting order, so I prefer to leave it as it is
    if !contracts_with_inconsistency.is_empty() {
        // todo I have to clone data above because of this retain. Rewrite this
        events.retain(|e| !contracts_with_inconsistency.contains(&e.contract_account_id));

        for contract in &contracts_with_inconsistency {
            contracts::mark_contract_inconsistent(
                pool,
                &AccountId::from_str(contract)?,
                &streamer_message.block.header,
                contracts,
            )
            .await?;
        }
    }

    models::chunked_insert(pool, &events).await
}

async fn collect_ft_for_shard(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    ft_balance_cache: &crate::FtBalanceCache,
    contracts: &crate::ActiveContracts,
    shard: &near_indexer_primitives::IndexerShard,
) -> anyhow::Result<Vec<CoinEvent>> {
    let mut events: Vec<CoinEvent> = vec![];

    let nep141_future = nep141_events::collect_nep141_events(
        json_rpc_client,
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        ft_balance_cache,
        contracts,
    );
    let legacy_contracts_future = legacy::collect_legacy(
        json_rpc_client,
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        ft_balance_cache,
        contracts,
    );
    let (nep141_events, legacy_events) = try_join!(nep141_future, legacy_contracts_future)?;

    events.extend(nep141_events);
    events.extend(legacy_events);
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
