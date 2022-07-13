use bigdecimal::BigDecimal;

use crate::models;
use near_lake_framework::near_indexer_primitives;

use super::event_types;

pub(crate) async fn store_nft_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shard: &near_indexer_primitives::IndexerShard,
    block_timestamp: u64,
    events_with_outcomes: &[(
        event_types::Nep171Event,
        &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    )],
) -> anyhow::Result<()> {
    let nft_events = compose_nft_db_events(events_with_outcomes, block_timestamp, &shard.shard_id);
    models::chunked_insert(pool, &nft_events).await?;

    Ok(())
}

fn compose_nft_db_events(
    events_with_outcomes: &[(
        event_types::Nep171Event,
        &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    )],
    block_timestamp: u64,
    shard_id: &near_indexer_primitives::types::ShardId,
) -> Vec<models::non_fungible_token_events::NonFungibleTokenEvent> {
    let mut nft_events = Vec::new();
    for (event, outcome) in events_with_outcomes {
        let contract_id = &outcome.receipt.receiver_id;
        match &event.event_kind {
            event_types::Nep171EventKind::NftMint(mint_events) => {
                for mint_event in mint_events {
                    let memo = mint_event.memo.clone().unwrap_or_else(|| "".to_string());
                    for token_id in &mint_event.token_ids {
                        nft_events.push(models::non_fungible_token_events::NonFungibleTokenEvent {
                            emitted_for_receipt_id: outcome.receipt.receipt_id.to_string(),
                            emitted_at_block_timestamp: BigDecimal::from(block_timestamp),
                            emitted_in_shard_id: BigDecimal::from(*shard_id),
                            emitted_index_of_event_entry_in_shard: nft_events.len() as i32,
                            emitted_by_contract_account_id: contract_id.to_string(),
                            token_id: token_id.escape_default().to_string(),
                            event_kind: "MINT".to_string(),
                            token_old_owner_account_id: "".to_string(),
                            token_new_owner_account_id: mint_event
                                .owner_id
                                .escape_default()
                                .to_string(),
                            token_authorized_account_id: "".to_string(),
                            event_memo: memo.escape_default().to_string(),
                        });
                    }
                }
            }
            event_types::Nep171EventKind::NftTransfer(transfer_events) => {
                for transfer_event in transfer_events {
                    let authorized_id = transfer_event
                        .authorized_id
                        .clone()
                        .unwrap_or_else(|| "".to_string());
                    let memo = transfer_event
                        .memo
                        .clone()
                        .unwrap_or_else(|| "".to_string());
                    for token_id in &transfer_event.token_ids {
                        nft_events.push(models::non_fungible_token_events::NonFungibleTokenEvent {
                            emitted_for_receipt_id: outcome.receipt.receipt_id.to_string(),
                            emitted_at_block_timestamp: BigDecimal::from(block_timestamp),
                            emitted_in_shard_id: BigDecimal::from(*shard_id),
                            emitted_index_of_event_entry_in_shard: nft_events.len() as i32,
                            emitted_by_contract_account_id: contract_id.to_string(),
                            token_id: token_id.escape_default().to_string(),
                            event_kind: "TRANSFER".to_string(),
                            token_old_owner_account_id: transfer_event
                                .old_owner_id
                                .escape_default()
                                .to_string(),
                            token_new_owner_account_id: transfer_event
                                .new_owner_id
                                .escape_default()
                                .to_string(),
                            token_authorized_account_id: authorized_id.escape_default().to_string(),
                            event_memo: memo.escape_default().to_string(),
                        });
                    }
                }
            }
            event_types::Nep171EventKind::NftBurn(burn_events) => {
                for burn_event in burn_events {
                    let authorized_id = &burn_event
                        .authorized_id
                        .clone()
                        .unwrap_or_else(|| "".to_string());
                    let memo = burn_event.memo.clone().unwrap_or_else(|| "".to_string());
                    for token_id in &burn_event.token_ids {
                        nft_events.push(models::non_fungible_token_events::NonFungibleTokenEvent {
                            emitted_for_receipt_id: outcome.receipt.receipt_id.to_string(),
                            emitted_at_block_timestamp: BigDecimal::from(block_timestamp),
                            emitted_in_shard_id: BigDecimal::from(*shard_id),
                            emitted_index_of_event_entry_in_shard: nft_events.len() as i32,
                            emitted_by_contract_account_id: contract_id.to_string(),
                            token_id: token_id.escape_default().to_string(),
                            event_kind: "BURN".to_string(),
                            token_old_owner_account_id: burn_event
                                .owner_id
                                .escape_default()
                                .to_string(),
                            token_new_owner_account_id: "".to_string(),
                            token_authorized_account_id: authorized_id.escape_default().to_string(),
                            event_memo: memo.escape_default().to_string(),
                        });
                    }
                }
            }
        }
    }
    nft_events
}
