use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use near_lake_framework::near_indexer_primitives::views::ExecutionStatusView;
use std::str::FromStr;

mod coin;
mod event_types;
pub(crate) mod events;
mod nft;

pub(crate) const CHUNK_SIZE_FOR_BATCH_INSERT: usize = 100;
pub(crate) const RETRY_COUNT: usize = 10;

pub(crate) enum Event {
    Nep141,
    Nep171,
    Aurora,
    RainbowBridge,
    TknNear,
    Wentokensir,
    WrapNear,
}

pub(crate) struct EventBase {
    pub event_index: BigDecimal,
    pub receipt_id: String,
    pub block_timestamp: BigDecimal,
    pub contract_account_id: near_primitives::types::AccountId,
    pub status: near_primitives::views::ExecutionStatusView,
}

pub(crate) fn get_base(
    event_type: Event,
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

fn get_status(status: &ExecutionStatusView) -> String {
    match status {
        ExecutionStatusView::Unknown => {
            tracing::warn!(target: crate::INDEXER, "Unknown execution status view",);
            "UNKNOWN"
        }
        ExecutionStatusView::Failure(_) => "FAILURE",
        ExecutionStatusView::SuccessValue(_) => "SUCCESS",
        ExecutionStatusView::SuccessReceiptId(_) => "SUCCESS",
    }
    .to_string()
}

fn compose_db_index(
    block_timestamp: u64,
    shard_id: &near_primitives::types::ShardId,
    event: Event,
    event_index: usize,
) -> anyhow::Result<BigDecimal> {
    let timestamp_millis: u128 = (block_timestamp as u128) / 1_000_000;
    // maybe it's better to have here 141, 171, but in this case we will not use most of the numbers and overflow our formula
    // we decided to have mac type index 10_000
    let event_type_index: u128 = match event {
        Event::Nep141 => 1,
        Event::Nep171 => 2,
        Event::RainbowBridge => 3,
        Event::TknNear => 4,
        Event::Wentokensir => 5,
        Event::WrapNear => 6,
        Event::Aurora => 7,
    };
    let db_index: u128 = timestamp_millis * 100_000_000_000 * 100_000_000_000
        + (*shard_id as u128) * 10_000_000
        + event_type_index * 10_000
        + (event_index as u128);
    Ok(BigDecimal::from_str(&db_index.to_string())?)
}
