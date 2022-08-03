use crate::db_adapters::ft_balance_utils;
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;

mod rainbow_bridge_events;
mod wrap_near_events;

pub(crate) use rainbow_bridge_events::store_rainbow_bridge;
pub(crate) use wrap_near_events::store_wrap_near;

struct EventBase {
    pub event_index: BigDecimal,
    pub receipt_id: String,
    pub block_timestamp: BigDecimal,
    pub contract_account_id: near_primitives::types::AccountId,
    pub status: near_primitives::views::ExecutionStatusView,
}

struct EventCustom {
    pub affected_id: near_primitives::types::AccountId,
    pub involved_id: Option<near_primitives::types::AccountId>,
    pub delta: BigDecimal,
    pub cause: String,
    pub memo: Option<String>,
}

fn get_base(
    event_type: super::events::Event,
    shard_id: &near_indexer_primitives::types::ShardId,
    starting_index: usize,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<EventBase> {
    Ok(EventBase {
        event_index: crate::db_adapters::compose_db_index(
            block_header.timestamp,
            shard_id,
            event_type,
            starting_index,
        )?,
        receipt_id: outcome.receipt.receipt_id.to_string(),
        block_timestamp: BigDecimal::from(block_header.timestamp),
        contract_account_id: outcome.execution_outcome.outcome.executor_id.clone(),
        status: outcome.execution_outcome.outcome.status.clone(),
    })
}

async fn build_event(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    ft_balance_cache: &crate::FtBalanceCache,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    base: EventBase,
    custom: EventCustom,
) -> anyhow::Result<CoinEvent> {
    let absolute_amount = ft_balance_utils::update_cache_and_get_balance(
        json_rpc_client,
        ft_balance_cache,
        &base.status,
        &block_header.prev_hash,
        base.contract_account_id.clone(),
        &custom.affected_id,
        &custom.delta,
    )
    .await?;

    // I calc it just to check the logic. Should be dropped in the end
    // actually they could be different if the block contains more than one operations with this account
    // but it's enough to check at least something
    let correct_value = ft_balance_utils::get_balance_from_rpc(
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
