use crate::db_adapters::{contracts, Event};
use crate::models;
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use futures::future::try_join_all;
use futures::try_join;
use near_lake_framework::near_indexer_primitives;
use near_primitives::types::AccountId;
use num_traits::Zero;
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
    contracts: &contracts::ContractsHelper,
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
    models::chunked_insert(pool, &events).await
}

pub(crate) async fn filter_inconsistent_events(
    coin_events: &mut Vec<CoinEvent>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    contracts: &contracts::ContractsHelper,
) -> anyhow::Result<()> {
    // At first, we can filter events with zero delta: they make no sense for us
    coin_events.retain(|event| !event.delta_amount.is_zero());
    // We go by all collected events (sorted historically) and check the latest balance for each affected user
    // (we go backwards and check only first entry for each user)
    let mut affected_account_ids = std::collections::HashSet::new();
    let mut inconsistent_contracts = std::collections::HashSet::new();
    for event in coin_events.iter().rev() {
        // We can mark contract as inconsistent during constructing events if we meet negative balance.
        // In this case, we don't need to check balance: it's 100% wrong.
        if (contracts
            .is_contract_inconsistent(&AccountId::from_str(&event.contract_account_id)?)
            .await
            || affected_account_ids.insert(event.affected_account_id.clone())
                && !balance_utils::is_balance_correct(
                    json_rpc_client,
                    block_header,
                    &event.contract_account_id,
                    &event.affected_account_id,
                    &event.absolute_amount,
                )
                .await?)
            && inconsistent_contracts.insert(event.contract_account_id.clone())
        {
            tracing::warn!(
                target: crate::LOGGING_PREFIX,
                "Inconsistent contract {} at block {}. Events collected:\n{:#?}",
                event.contract_account_id,
                block_header.height,
                coin_events,
            );
            contracts
                .mark_contract_inconsistent(models::contracts::Contract {
                    contract_account_id: event.contract_account_id.clone(),
                    standard: event.standard.clone(),
                    // If the contract was created earlier, this value would be ignored
                    first_event_at_timestamp: event.block_timestamp.clone(),
                    // If the contract was created earlier, this value would be ignored
                    first_event_at_block_height: event.block_height.clone(),
                    inconsistency_found_at_timestamp: Some(event.block_timestamp.clone()),
                    inconsistency_found_at_block_height: Some(event.block_height.clone()),
                })
                .await?;
        }
    }

    if !inconsistent_contracts.is_empty() {
        coin_events.retain(|event| !inconsistent_contracts.contains(&event.contract_account_id));
    }
    Ok(())
}

pub(crate) async fn register_new_contracts(
    coin_events: &mut [CoinEvent],
    contracts: &contracts::ContractsHelper,
) -> anyhow::Result<()> {
    let mut new_contracts = std::collections::HashSet::new();
    for event in coin_events.iter().rev() {
        if new_contracts.insert(event.contract_account_id.clone()) {
            contracts
                .try_register_contract(models::contracts::Contract {
                    contract_account_id: event.contract_account_id.clone(),
                    standard: event.standard.clone(),
                    // If the contract was created earlier, this value would be ignored
                    first_event_at_timestamp: event.block_timestamp.clone(),
                    // If the contract was created earlier, this value would be ignored
                    first_event_at_block_height: event.block_height.clone(),
                    inconsistency_found_at_timestamp: None,
                    inconsistency_found_at_block_height: None,
                })
                .await?;
        }
    }
    Ok(())
}

pub(crate) fn enumerate_events(
    ft_events: &mut [crate::models::coin_events::CoinEvent],
    shard_id: &near_indexer_primitives::types::ShardId,
    timestamp: u64,
    event_type: &Event,
) -> anyhow::Result<()> {
    for (index, event) in ft_events.iter_mut().enumerate() {
        event.event_index =
            crate::db_adapters::compose_db_index(timestamp, shard_id, event_type, index)?;
    }
    Ok(())
}

async fn collect_ft_for_shard(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    ft_balance_cache: &crate::FtBalanceCache,
    contracts: &contracts::ContractsHelper,
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
    contracts: &contracts::ContractsHelper,
) -> anyhow::Result<CoinEvent> {
    let absolute_amount = balance_utils::update_cache_and_get_balance(
        json_rpc_client,
        ft_balance_cache,
        &base,
        block_header,
        &custom.affected_id,
        &custom.delta,
        contracts,
    )
    .await?;

    Ok(CoinEvent {
        event_index: BigDecimal::zero(), // initialized later
        standard: base.standard,
        receipt_id: base.receipt_id,
        block_height: base.block_height,
        block_timestamp: base.block_timestamp,
        contract_account_id: base.contract_account_id.to_string(),
        affected_account_id: custom.affected_id.to_string(),
        involved_account_id: custom.involved_id.map(|id| id.to_string()),
        delta_amount: custom.delta,
        absolute_amount,
        // coin_id: "".to_string(),
        cause: custom.cause,
        status: crate::db_adapters::get_status(&base.status),
        event_memo: custom.memo,
    })
}
