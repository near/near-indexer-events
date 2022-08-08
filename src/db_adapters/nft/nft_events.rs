use bigdecimal::BigDecimal;

use crate::db_adapters::event_types::Nep171Event;
use crate::db_adapters::Event;
use crate::db_adapters::{compose_db_index, get_status};
use crate::models::nft_events::NftEvent;
use near_lake_framework::near_indexer_primitives;

use crate::db_adapters::event_types;

pub(crate) async fn store_nft_events(
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<Vec<NftEvent>> {
    let mut res = Vec::new();
    for outcome in receipt_execution_outcomes {
        for events in crate::db_adapters::events::extract_events(outcome) {
            if let event_types::NearEvent::Nep171(nft_events) = events {
                compose_nft_db_events(
                    &mut res,
                    &nft_events,
                    outcome,
                    block_header.timestamp,
                    shard_id,
                )?;
            }
        }
    }
    Ok(res)
}

fn compose_nft_db_events(
    nft_events: &mut Vec<NftEvent>,
    events: &Nep171Event,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    block_timestamp: u64,
    shard_id: &near_indexer_primitives::types::ShardId,
) -> anyhow::Result<()> {
    let contract_id = &outcome.receipt.receiver_id;
    match &events.event_kind {
        event_types::Nep171EventKind::NftMint(mint_events) => {
            for mint_event in mint_events {
                for token_id in &mint_event.token_ids {
                    nft_events.push(NftEvent {
                        event_index: compose_db_index(
                            block_timestamp,
                            shard_id,
                            Event::Nep171,
                            nft_events.len(),
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_timestamp),
                        contract_account_id: contract_id.to_string(),
                        token_id: token_id.escape_default().to_string(),
                        cause: "MINT".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        old_owner_account_id: None,
                        new_owner_account_id: Some(
                            mint_event.owner_id.escape_default().to_string(),
                        ),
                        authorized_account_id: None,
                        event_memo: mint_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
        }
        event_types::Nep171EventKind::NftTransfer(transfer_events) => {
            for transfer_event in transfer_events {
                for token_id in &transfer_event.token_ids {
                    nft_events.push(NftEvent {
                        event_index: compose_db_index(
                            block_timestamp,
                            shard_id,
                            Event::Nep171,
                            nft_events.len(),
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_timestamp),
                        contract_account_id: contract_id.to_string(),
                        token_id: token_id.escape_default().to_string(),
                        cause: "TRANSFER".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        old_owner_account_id: Some(
                            transfer_event.old_owner_id.escape_default().to_string(),
                        ),
                        new_owner_account_id: Some(
                            transfer_event.new_owner_id.escape_default().to_string(),
                        ),
                        authorized_account_id: transfer_event
                            .authorized_id
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                        event_memo: transfer_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
        }
        event_types::Nep171EventKind::NftBurn(burn_events) => {
            for burn_event in burn_events {
                for token_id in &burn_event.token_ids {
                    nft_events.push(NftEvent {
                        event_index: compose_db_index(
                            block_timestamp,
                            shard_id,
                            Event::Nep171,
                            nft_events.len(),
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_timestamp),
                        contract_account_id: contract_id.to_string(),
                        token_id: token_id.escape_default().to_string(),
                        cause: "BURN".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        old_owner_account_id: Some(
                            burn_event.owner_id.escape_default().to_string(),
                        ),
                        new_owner_account_id: None,
                        authorized_account_id: burn_event
                            .authorized_id
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                        event_memo: burn_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
        }
    }
    Ok(())
}
