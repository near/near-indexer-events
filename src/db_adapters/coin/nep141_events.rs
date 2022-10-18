use crate::db_adapters::event_types::Nep141Event;
use crate::db_adapters::{coin, events, get_status};
use crate::db_adapters::{contracts, Event};
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use num_traits::Zero;
use std::ops::Mul;
use std::str::FromStr;

use crate::db_adapters::coin::{balance_utils, FT};
use crate::db_adapters::event_types;

pub(crate) async fn collect_nep141_events(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    ft_balance_cache: &crate::FtBalanceCache,
    contracts: &contracts::ContractsHelper,
) -> anyhow::Result<Vec<CoinEvent>> {
    let mut res = Vec::new();
    for outcome in receipt_execution_outcomes {
        let events = events::extract_events(outcome);
        if !events.is_empty()
            && contracts
                .is_contract_inconsistent(&outcome.receipt.receiver_id)
                .await
        {
            continue;
        }
        for event in events {
            if let event_types::NearEvent::Nep141(ft_events) = event {
                res.extend(
                    compose_db_events(
                        json_rpc_client,
                        &ft_events,
                        outcome,
                        block_header,
                        ft_balance_cache,
                    )
                    .await?,
                );
            }
        }
    }
    if !res.is_empty() {
        coin::register_new_contracts(&mut res, contracts).await?;
        coin::filter_inconsistent_events(&mut res, json_rpc_client, block_header, contracts)
            .await?;
        coin::enumerate_events(&mut res, shard_id, block_header.timestamp, &Event::Nep141)?;
    }

    Ok(res)
}

async fn compose_db_events(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    events: &Nep141Event,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    ft_balance_cache: &crate::FtBalanceCache,
) -> anyhow::Result<Vec<CoinEvent>> {
    let mut ft_events = Vec::new();
    let contract_id = &outcome.receipt.receiver_id;
    match &events.event_kind {
        event_types::Nep141EventKind::FtMint(mint_events) => {
            for mint_event in mint_events {
                let delta_amount = BigDecimal::from_str(&mint_event.amount)?;
                let absolute_amount = balance_utils::update_cache_and_get_balance(
                    json_rpc_client,
                    ft_balance_cache,
                    &outcome.execution_outcome.outcome.status,
                    block_header,
                    contract_id.clone(),
                    &mint_event.owner_id,
                    &delta_amount,
                )
                .await?;
                ft_events.push(CoinEvent {
                    event_index: BigDecimal::zero(), // initialized later
                    standard: FT.to_string(),
                    receipt_id: outcome.receipt.receipt_id.to_string(),
                    block_height: BigDecimal::from(block_header.height),
                    block_timestamp: BigDecimal::from(block_header.timestamp),
                    contract_account_id: contract_id.to_string(),
                    affected_account_id: mint_event.owner_id.escape_default().to_string(),
                    involved_account_id: None,
                    delta_amount,
                    absolute_amount,
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
                let absolute_amount = balance_utils::update_cache_and_get_balance(
                    json_rpc_client,
                    ft_balance_cache,
                    &outcome.execution_outcome.outcome.status,
                    block_header,
                    contract_id.clone(),
                    &transfer_event.old_owner_id,
                    &delta_amount,
                )
                .await?;
                ft_events.push(CoinEvent {
                    event_index: BigDecimal::zero(), // initialized later
                    standard: FT.to_string(),
                    receipt_id: outcome.receipt.receipt_id.to_string(),
                    block_height: BigDecimal::from(block_header.height),
                    block_timestamp: BigDecimal::from(block_header.timestamp),
                    contract_account_id: contract_id.to_string(),
                    affected_account_id: transfer_event.old_owner_id.escape_default().to_string(),
                    involved_account_id: Some(
                        transfer_event.new_owner_id.escape_default().to_string(),
                    ),
                    delta_amount,
                    absolute_amount,
                    // coin_id: "".to_string(),
                    cause: "TRANSFER".to_string(),
                    status: get_status(&outcome.execution_outcome.outcome.status),
                    event_memo: transfer_event
                        .memo
                        .as_ref()
                        .map(|s| s.escape_default().to_string()),
                });

                let delta_amount = BigDecimal::from_str(&transfer_event.amount)?;
                let absolute_amount = balance_utils::update_cache_and_get_balance(
                    json_rpc_client,
                    ft_balance_cache,
                    &outcome.execution_outcome.outcome.status,
                    block_header,
                    contract_id.clone(),
                    &transfer_event.new_owner_id,
                    &delta_amount,
                )
                .await?;
                ft_events.push(CoinEvent {
                    event_index: BigDecimal::zero(), // initialized later
                    standard: FT.to_string(),
                    receipt_id: outcome.receipt.receipt_id.to_string(),
                    block_height: BigDecimal::from(block_header.height),
                    block_timestamp: BigDecimal::from(block_header.timestamp),
                    contract_account_id: contract_id.to_string(),
                    affected_account_id: transfer_event.new_owner_id.escape_default().to_string(),
                    involved_account_id: Some(
                        transfer_event.old_owner_id.escape_default().to_string(),
                    ),
                    delta_amount,
                    absolute_amount,
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
                let absolute_amount = balance_utils::update_cache_and_get_balance(
                    json_rpc_client,
                    ft_balance_cache,
                    &outcome.execution_outcome.outcome.status,
                    block_header,
                    contract_id.clone(),
                    &burn_event.owner_id,
                    &delta_amount,
                )
                .await?;
                ft_events.push(CoinEvent {
                    event_index: BigDecimal::zero(), // initialized later
                    standard: FT.to_string(),
                    receipt_id: outcome.receipt.receipt_id.to_string(),
                    block_height: BigDecimal::from(block_header.height),
                    block_timestamp: BigDecimal::from(block_header.timestamp),
                    contract_account_id: contract_id.to_string(),
                    affected_account_id: burn_event.owner_id.escape_default().to_string(),
                    involved_account_id: None,
                    delta_amount,
                    absolute_amount,
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

    Ok(ft_events)
}
