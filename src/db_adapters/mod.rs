use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives::views::ExecutionStatusView;
use std::str::FromStr;

mod coin_events;
mod event_types;
pub(crate) mod events;
mod ft_balance_utils;
mod nft_events;
mod wrap_near_events;

pub(crate) const CHUNK_SIZE_FOR_BATCH_INSERT: usize = 100;
pub(crate) const RETRY_COUNT: usize = 10;

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
    event: events::Event,
    event_index: usize,
) -> anyhow::Result<BigDecimal> {
    let timestamp_millis: u128 = (block_timestamp as u128) / 1_000_000;
    // maybe it's better to have here 141, 171, but in this case we will not use most of the numbers and overflow our formula
    // we decided to have mac type index 10_000
    let event_type_index: u128 = match event {
        events::Event::Nep141 => 1,
        events::Event::Nep171 => 2,
        events::Event::WrapNear => 3,
    };
    let db_index: u128 = timestamp_millis * 100_000_000_000 * 100_000_000_000
        + (*shard_id as u128) * 10_000_000
        + event_type_index * 10_000
        + (event_index as u128);
    Ok(BigDecimal::from_str(&db_index.to_string())?)
}
