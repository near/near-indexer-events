use crate::db_adapters::events::Event;
use crate::db_adapters::{compose_db_index, get_status};
use crate::models;
use bigdecimal::BigDecimal;
use cached::Cached;
use near_lake_framework::near_indexer_primitives;
use std::ops::{Add, Mul};
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
                    let absolute_amount = update_cache_and_get_balance(
                        json_rpc_client,
                        ft_balance_cache,
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
                    let absolute_amount = update_cache_and_get_balance(
                        json_rpc_client,
                        ft_balance_cache,
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
                    let absolute_amount = update_cache_and_get_balance(
                        json_rpc_client,
                        ft_balance_cache,
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
                    let absolute_amount = update_cache_and_get_balance(
                        json_rpc_client,
                        ft_balance_cache,
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

async fn update_cache_and_get_balance(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    ft_balance_cache: &crate::FtBalanceCache,
    prev_block_hash: &near_indexer_primitives::CryptoHash,
    contract_id: near_primitives::types::AccountId,
    account_id: &str,
    delta_amount: &BigDecimal,
) -> anyhow::Result<BigDecimal> {
    let account_with_contract = crate::AccountWithContract {
        account_id: near_primitives::types::AccountId::from_str(account_id)?,
        contract_account_id: contract_id,
    };
    let prev_absolute_amount = BigDecimal::from_str(
        &get_balance_retriable(
            account_with_contract.clone(),
            prev_block_hash,
            ft_balance_cache,
            json_rpc_client,
        )
        .await?
        .to_string(),
    )?;
    let absolute_amount = prev_absolute_amount.add(delta_amount);
    save_latest_ft_balance(
        account_with_contract,
        absolute_amount.to_string().parse::<u128>()?,
        ft_balance_cache,
    )
    .await;
    Ok(absolute_amount)
}

async fn save_latest_ft_balance(
    account_with_contract: crate::AccountWithContract,
    balance: near_indexer_primitives::types::Balance,
    ft_balance_cache: &crate::FtBalanceCache,
) {
    let mut balances_cache_lock = ft_balance_cache.lock().await;
    balances_cache_lock.cache_set(account_with_contract, balance);
    drop(balances_cache_lock);
}

async fn get_balance_retriable(
    account_with_contract: crate::AccountWithContract,
    block_hash: &near_indexer_primitives::CryptoHash,
    ft_balance_cache: &crate::FtBalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<near_primitives::types::Balance> {
    let mut interval = crate::INTERVAL;
    let mut retry_attempt = 0usize;

    loop {
        if retry_attempt == crate::db_adapters::RETRY_COUNT {
            anyhow::bail!(
                "Failed to perform query to RPC after {} attempts. Stop trying.\nAccount {}, block_hash {}",
                crate::db_adapters::RETRY_COUNT,
                account_with_contract.account_id.to_string(),
                block_hash.to_string()
            );
        }
        retry_attempt += 1;

        match get_balance(
            account_with_contract.clone(),
            block_hash,
            ft_balance_cache,
            json_rpc_client,
        )
        .await
        {
            Ok(res) => return Ok(res),
            Err(err) => {
                tracing::error!(
                    target: crate::INDEXER,
                    "Failed to request account view details from RPC for account {}, block_hash {}.{}\n Retrying in {} milliseconds...",
                    account_with_contract.account_id.to_string(),
                    block_hash.to_string(),
                    err,
                    interval.as_millis(),
                );
                tokio::time::sleep(interval).await;
                if interval < crate::MAX_DELAY_TIME {
                    interval *= 2;
                }
            }
        }
    }
}

async fn get_balance(
    account_with_contract: crate::AccountWithContract,
    block_hash: &near_indexer_primitives::CryptoHash,
    ft_balance_cache: &crate::FtBalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<near_primitives::types::Balance> {
    let mut balances_cache_lock = ft_balance_cache.lock().await;
    let result = match balances_cache_lock.cache_get(&account_with_contract) {
        None => {
            let request = crate::rpc_helpers::get_function_call_request(
                block_hash,
                account_with_contract.contract_account_id.clone(),
                "ft_balance_of",
                serde_json::json!({ "account_id": account_with_contract.account_id }),
            );
            let response = crate::rpc_helpers::wrapped_call(
                json_rpc_client,
                request,
                block_hash,
                &account_with_contract.contract_account_id,
            )
            .await?;
            let balance = serde_json::from_slice::<String>(&response.result)?.parse::<u128>()?;

            balances_cache_lock.cache_set(account_with_contract.clone(), balance);

            Ok(balance)
        }
        Some(balance) => Ok(*balance),
    };
    drop(balances_cache_lock);
    result
}
