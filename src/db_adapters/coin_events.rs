use crate::db_adapters::events::Event;
use crate::db_adapters::{compose_db_index, ft_balance_utils, get_status};
use crate::models;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use std::ops::Mul;
use std::str::FromStr;

use super::event_types;

pub(crate) struct FtEvent {
    pub events: event_types::Nep141Event,
    pub outcome: near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    pub starting_index: usize,
}

pub(crate) async fn store_ft_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    shard: &near_indexer_primitives::IndexerShard,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    events: &[FtEvent],
    ft_balance_cache: &crate::FtBalanceCache,
) -> anyhow::Result<()> {
    let ft_events = compose_ft_db_events(
        json_rpc_client,
        events,
        block_header,
        &shard.shard_id,
        ft_balance_cache,
    )
    .await?;
    models::chunked_insert(pool, &ft_events).await?;

    Ok(())
}

async fn compose_ft_db_events(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    events: &[FtEvent],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    shard_id: &near_indexer_primitives::types::ShardId,
    ft_balance_cache: &crate::FtBalanceCache,
) -> anyhow::Result<Vec<models::coin_events::CoinEvent>> {
    let mut ft_events = Vec::new();
    for FtEvent {
        events,
        outcome,
        starting_index,
    } in events
    {
        let contract_id = &outcome.receipt.receiver_id;
        match &events.event_kind {
            event_types::Nep141EventKind::FtMint(mint_events) => {
                for mint_event in mint_events {
                    let delta_amount = BigDecimal::from_str(&mint_event.amount)?;
                    let absolute_amount = ft_balance_utils::update_cache_and_get_balance(
                        json_rpc_client,
                        ft_balance_cache,
                        &outcome.execution_outcome.outcome.status,
                        &block_header.prev_hash,
                        contract_id.clone(),
                        &mint_event.owner_id,
                        &delta_amount,
                    )
                    .await?;
                    ft_events.push(models::coin_events::CoinEvent {
                        event_index: compose_db_index(
                            block_header.timestamp,
                            shard_id,
                            Event::Nep141,
                            starting_index + ft_events.len(),
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_header.timestamp),
                        contract_account_id: contract_id.to_string(),
                        affected_account_id: mint_event.owner_id.escape_default().to_string(),
                        involved_account_id: None,
                        delta_amount,
                        absolute_amount,
                        // standard: "nep141".to_string(),
                        // coin_id: "".to_string(),
                        cause: "MINT".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        event_memo: mint_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
            event_types::Nep141EventKind::FtTransfer(transfer_events) => {
                for transfer_event in transfer_events {
                    let delta_amount =
                        BigDecimal::from_str(&transfer_event.amount)?.mul(BigDecimal::from(-1));
                    let absolute_amount = ft_balance_utils::update_cache_and_get_balance(
                        json_rpc_client,
                        ft_balance_cache,
                        &outcome.execution_outcome.outcome.status,
                        &block_header.prev_hash,
                        contract_id.clone(),
                        &transfer_event.old_owner_id,
                        &delta_amount,
                    )
                    .await?;
                    ft_events.push(models::coin_events::CoinEvent {
                        event_index: compose_db_index(
                            block_header.timestamp,
                            shard_id,
                            Event::Nep141,
                            starting_index + ft_events.len(),
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_header.timestamp),
                        contract_account_id: contract_id.to_string(),
                        affected_account_id: transfer_event
                            .old_owner_id
                            .escape_default()
                            .to_string(),
                        involved_account_id: Some(
                            transfer_event.new_owner_id.escape_default().to_string(),
                        ),
                        delta_amount,
                        absolute_amount,
                        // standard: "".to_string(),
                        // coin_id: "".to_string(),
                        cause: "TRANSFER".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        event_memo: transfer_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });

                    let delta_amount = BigDecimal::from_str(&transfer_event.amount)?;
                    let absolute_amount = ft_balance_utils::update_cache_and_get_balance(
                        json_rpc_client,
                        ft_balance_cache,
                        &outcome.execution_outcome.outcome.status,
                        &block_header.prev_hash,
                        contract_id.clone(),
                        &transfer_event.new_owner_id,
                        &delta_amount,
                    )
                    .await?;
                    ft_events.push(models::coin_events::CoinEvent {
                        event_index: compose_db_index(
                            block_header.timestamp,
                            shard_id,
                            Event::Nep141,
                            starting_index + ft_events.len(),
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_header.timestamp),
                        contract_account_id: contract_id.to_string(),
                        affected_account_id: transfer_event
                            .new_owner_id
                            .escape_default()
                            .to_string(),
                        involved_account_id: Some(
                            transfer_event.old_owner_id.escape_default().to_string(),
                        ),
                        delta_amount,
                        absolute_amount,
                        // standard: "".to_string(),
                        // coin_id: "".to_string(),
                        cause: "TRANSFER".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        event_memo: transfer_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
            event_types::Nep141EventKind::FtBurn(burn_events) => {
                for burn_event in burn_events {
                    let delta_amount =
                        BigDecimal::from_str(&burn_event.amount)?.mul(BigDecimal::from(-1));
                    let absolute_amount = ft_balance_utils::update_cache_and_get_balance(
                        json_rpc_client,
                        ft_balance_cache,
                        &outcome.execution_outcome.outcome.status,
                        &block_header.prev_hash,
                        contract_id.clone(),
                        &burn_event.owner_id,
                        &delta_amount,
                    )
                    .await?;
                    ft_events.push(models::coin_events::CoinEvent {
                        event_index: compose_db_index(
                            block_header.timestamp,
                            shard_id,
                            Event::Nep141,
                            starting_index + ft_events.len(),
                        )?,
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_timestamp: BigDecimal::from(block_header.timestamp),
                        contract_account_id: contract_id.to_string(),
                        affected_account_id: burn_event.owner_id.escape_default().to_string(),
                        involved_account_id: None,
                        delta_amount,
                        absolute_amount,
                        // standard: "".to_string(),
                        // coin_id: "".to_string(),
                        cause: "BURN".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        event_memo: burn_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
        }
    }
    Ok(ft_events)
}
