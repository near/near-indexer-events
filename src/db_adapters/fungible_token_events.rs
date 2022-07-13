use crate::models;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;

use super::event_types;

pub(crate) async fn store_ft_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shard: &near_indexer_primitives::IndexerShard,
    block_timestamp: u64,
    events_with_outcomes: &[(
        event_types::Nep141Event,
        &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    )],
) -> anyhow::Result<()> {
    let ft_events = compose_ft_db_events(events_with_outcomes, block_timestamp, &shard.shard_id);
    models::chunked_insert(pool, &ft_events).await?;

    Ok(())
}

fn compose_ft_db_events(
    events_with_outcomes: &[(
        event_types::Nep141Event,
        &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    )],
    block_timestamp: u64,
    shard_id: &near_indexer_primitives::types::ShardId,
) -> Vec<models::fungible_token_events::FungibleTokenEvent> {
    let mut ft_events = Vec::new();
    for (event, outcome) in events_with_outcomes {
        let contract_id = &outcome.receipt.receiver_id;
        match &event.event_kind {
            event_types::Nep141EventKind::FtMint(mint_events) => {
                for mint_event in mint_events {
                    ft_events.push(models::fungible_token_events::FungibleTokenEvent {
                        emitted_for_receipt_id: outcome.receipt.receipt_id.to_string(),
                        emitted_at_block_timestamp: BigDecimal::from(block_timestamp),
                        emitted_in_shard_id: BigDecimal::from(*shard_id),
                        emitted_index_of_event_entry_in_shard: ft_events.len() as i32,
                        emitted_by_contract_account_id: contract_id.to_string(),
                        amount: mint_event.amount.to_string(),
                        event_kind: "MINT".to_string(),
                        token_old_owner_account_id: "".to_string(),
                        token_new_owner_account_id: mint_event
                            .owner_id
                            .escape_default()
                            .to_string(),
                        event_memo: mint_event
                            .memo
                            .clone()
                            .unwrap_or_else(|| "".to_string())
                            .escape_default()
                            .to_string(),
                    });
                }
            }
            event_types::Nep141EventKind::FtTransfer(transfer_events) => {
                for transfer_event in transfer_events {
                    ft_events.push(models::fungible_token_events::FungibleTokenEvent {
                        emitted_for_receipt_id: outcome.receipt.receipt_id.to_string(),
                        emitted_at_block_timestamp: BigDecimal::from(block_timestamp),
                        emitted_in_shard_id: BigDecimal::from(*shard_id),
                        emitted_index_of_event_entry_in_shard: ft_events.len() as i32,
                        emitted_by_contract_account_id: contract_id.to_string(),
                        amount: transfer_event.amount.to_string(),
                        event_kind: "TRANSFER".to_string(),
                        token_old_owner_account_id: transfer_event
                            .old_owner_id
                            .escape_default()
                            .to_string(),
                        token_new_owner_account_id: transfer_event
                            .new_owner_id
                            .escape_default()
                            .to_string(),
                        event_memo: transfer_event
                            .memo
                            .clone()
                            .unwrap_or_else(|| "".to_string())
                            .escape_default()
                            .to_string(),
                    });
                }
            }
            event_types::Nep141EventKind::FtBurn(burn_events) => {
                for burn_event in burn_events {
                    ft_events.push(models::fungible_token_events::FungibleTokenEvent {
                        emitted_for_receipt_id: outcome.receipt.receipt_id.to_string(),
                        emitted_at_block_timestamp: BigDecimal::from(block_timestamp),
                        emitted_in_shard_id: BigDecimal::from(*shard_id),
                        emitted_index_of_event_entry_in_shard: ft_events.len() as i32,
                        emitted_by_contract_account_id: contract_id.to_string(),
                        amount: burn_event.amount.to_string(),
                        event_kind: "BURN".to_string(),
                        token_old_owner_account_id: burn_event
                            .owner_id
                            .escape_default()
                            .to_string(),
                        token_new_owner_account_id: "".to_string(),
                        event_memo: burn_event
                            .memo
                            .clone()
                            .unwrap_or_else(|| "".to_string())
                            .escape_default()
                            .to_string(),
                    });
                }
            }
        }
    }
    ft_events
}
