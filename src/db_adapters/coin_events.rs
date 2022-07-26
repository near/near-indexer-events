use crate::db_adapters::events::Event;
use crate::db_adapters::{compose_db_index, get_status};
use crate::models;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use num_traits::Zero;
use std::str::FromStr;

use super::event_types;

pub(crate) struct FtEvent {
    pub events: event_types::Nep141Event,
    pub outcome: near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    pub starting_index: usize,
}

pub(crate) async fn store_ft_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shard: &near_indexer_primitives::IndexerShard,
    block_timestamp: u64,
    events: &[FtEvent],
) -> anyhow::Result<()> {
    let ft_events = compose_ft_db_events(events, block_timestamp, &shard.shard_id)?;
    models::chunked_insert(pool, &ft_events).await?;

    Ok(())
}

fn compose_ft_db_events(
    events: &[FtEvent],
    block_timestamp: u64,
    shard_id: &near_indexer_primitives::types::ShardId,
) -> anyhow::Result<Vec<models::coin_events::CoinEvent>> {
    let mut ft_events = Vec::new();
    for FtEvent {
        events,
        outcome,
        mut starting_index,
    } in events
    {
        let contract_id = &outcome.receipt.receiver_id;
        match &events.event_kind {
            event_types::Nep141EventKind::FtMint(mint_events) => {
                for mint_event in mint_events {
                    ft_events.push(models::coin_events::CoinEvent {
                        event_index: compose_db_index(
                            block_timestamp,
                            shard_id,
                            Event::Nep141,
                            starting_index,
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_timestamp),
                        contract_account_id: contract_id.to_string(),
                        affected_account_id: mint_event.owner_id.escape_default().to_string(),
                        involved_account_id: None,
                        delta_amount: BigDecimal::from_str(&mint_event.amount)?,
                        absolute_amount: BigDecimal::zero(), // todo
                        // standard: "nep141".to_string(),
                        // coin_id: "".to_string(),
                        cause: "MINT".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        event_memo: mint_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                    starting_index += 1;
                }
            }
            event_types::Nep141EventKind::FtTransfer(transfer_events) => {
                for transfer_event in transfer_events {
                    let negative_delta = -transfer_event.amount.parse::<i128>()?;
                    ft_events.push(models::coin_events::CoinEvent {
                        event_index: compose_db_index(
                            block_timestamp,
                            shard_id,
                            Event::Nep141,
                            starting_index,
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_timestamp),
                        contract_account_id: contract_id.to_string(),
                        affected_account_id: transfer_event
                            .old_owner_id
                            .escape_default()
                            .to_string(),
                        involved_account_id: Some(
                            transfer_event.new_owner_id.escape_default().to_string(),
                        ),
                        delta_amount: BigDecimal::from_str(&negative_delta.to_string())?,
                        absolute_amount: BigDecimal::zero(), // todo
                        // standard: "".to_string(),
                        // coin_id: "".to_string(),
                        cause: "TRANSFER".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        event_memo: transfer_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                    starting_index += 1;

                    ft_events.push(models::coin_events::CoinEvent {
                        event_index: compose_db_index(
                            block_timestamp,
                            shard_id,
                            Event::Nep141,
                            starting_index,
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_timestamp),
                        contract_account_id: contract_id.to_string(),
                        affected_account_id: transfer_event
                            .new_owner_id
                            .escape_default()
                            .to_string(),
                        involved_account_id: Some(
                            transfer_event.old_owner_id.escape_default().to_string(),
                        ),
                        delta_amount: BigDecimal::from_str(&transfer_event.amount)?,
                        absolute_amount: BigDecimal::zero(), // todo
                        // standard: "".to_string(),
                        // coin_id: "".to_string(),
                        cause: "TRANSFER".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        event_memo: transfer_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                    starting_index += 1;
                }
            }
            event_types::Nep141EventKind::FtBurn(burn_events) => {
                for burn_event in burn_events {
                    ft_events.push(models::coin_events::CoinEvent {
                        event_index: compose_db_index(
                            block_timestamp,
                            shard_id,
                            Event::Nep141,
                            starting_index,
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_timestamp),
                        contract_account_id: contract_id.to_string(),
                        affected_account_id: burn_event.owner_id.escape_default().to_string(),
                        involved_account_id: None,
                        delta_amount: BigDecimal::from_str(&burn_event.amount)?,
                        absolute_amount: BigDecimal::zero(), // todo
                        // standard: "".to_string(),
                        // coin_id: "".to_string(),
                        cause: "BURN".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        event_memo: burn_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                    starting_index += 1;
                }
            }
        }
    }
    Ok(ft_events)
}
