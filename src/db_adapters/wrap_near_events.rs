use crate::db_adapters::{events, ft_balance_utils};
use crate::models;
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use near_primitives::types::AccountId;
use near_primitives::views::{ActionView, ReceiptEnumView};
use serde::{Deserialize, Serialize};
use std::ops::Mul;
use std::str::FromStr;

/// MINT
// near_deposit
// And parse the output logs. It should contain `Deposit {} NEAR to {}`
// Note: we can't look at the attached_deposit, because the function decreases this amount with storage costs (non-constant value)

/// TRANSFER
// ft_transfer ft_transfer_call ft_resolve_transfer

/// BURN
// near_withdraw
// with parameter amount
pub(crate) async fn store_wrap_near(
    pool: &sqlx::Pool<sqlx::Postgres>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    ft_balance_cache: &crate::FtBalanceCache,
) -> anyhow::Result<()> {
    let mut events: Vec<CoinEvent> = vec![];

    for outcome in receipt_execution_outcomes {
        if outcome.execution_outcome.outcome.executor_id != AccountId::from_str("wrap.near")? {
            continue;
        }
        if let ReceiptEnumView::Action { actions, .. } = &outcome.receipt.receipt {
            for action in actions {
                if let ActionView::FunctionCall {
                    method_name,
                    args,
                    deposit,
                    ..
                } = action
                {
                    eprintln!(
                        "{} {}\n{}",
                        method_name,
                        deposit,
                        &outcome.execution_outcome.outcome.logs.join("\n")
                    );

                    if let Ok(decoded_args) = base64::decode(args) {
                        if let Ok(args_json) =
                            serde_json::from_slice::<serde_json::Value>(&decoded_args)
                        {
                            eprintln!("wow args json {}", args_json);
                        }
                    }

                    if method_name == "storage_deposit" {
                        // ignore that
                    } else if method_name == "near_deposit" {
                        panic!("not implemented yet");
                    } else if method_name == "near_withdraw" {
                        panic!("not implemented");
                    } else if method_name == "ft_transfer" {
                        // case without 2fa
                        // https://explorer.near.org/transactions/EXuc2SVXfvexPxNggo5XdQD9JKABfv9i7LwdUUTX4ySG#3npphttR2J4EmxMrB8NGSu3icjEhaqiWS5vP5FVTTfjJ
                        // outcome.receipt.predecessor_id is sender
                        // outcome.receipt.receiver_id is contract id
                        // args.receiver_id is receiver

                        events.extend(
                            handle_ft_transfer(
                                json_rpc_client,
                                ft_balance_cache,
                                shard_id,
                                events.len(),
                                outcome,
                                block_header,
                                args,
                            )
                            .await?,
                        );
                    } else {
                        panic!("new method");
                    }
                }
            }
        }
    }

    models::chunked_insert(pool, &events).await?;
    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FtTransfer {
    pub receiver_id: AccountId,
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

// case without 2fa
// block 68814912, receipt 3npphttR2J4EmxMrB8NGSu3icjEhaqiWS5vP5FVTTfjJ
// https://explorer.near.org/transactions/EXuc2SVXfvexPxNggo5XdQD9JKABfv9i7LwdUUTX4ySG#3npphttR2J4EmxMrB8NGSu3icjEhaqiWS5vP5FVTTfjJ
// outcome.receipt.predecessor_id is sender
// outcome.receipt.receiver_id is contract id
// args.receiver_id is receiver
async fn handle_ft_transfer(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    ft_balance_cache: &crate::FtBalanceCache,
    shard_id: &near_indexer_primitives::types::ShardId,
    starting_index: usize,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    args: &str,
) -> anyhow::Result<Vec<CoinEvent>> {
    let decoded_args = base64::decode(args)?;
    // {"amount":"4900000000000000000000000","msg":null,"receiver_id":"spoiler.near"}
    let ft_transfer_args = serde_json::from_slice::<FtTransfer>(&decoded_args)?;

    let contract_id = outcome.receipt.receiver_id.clone();
    let sender_id = outcome.receipt.predecessor_id.clone();
    let receiver_id = ft_transfer_args.receiver_id;

    // outcome.receipt.predecessor_id

    let sender_delta_amount =
        BigDecimal::from_str(&ft_transfer_args.amount)?.mul(BigDecimal::from(-1));
    let sender_absolute_amount = ft_balance_utils::update_cache_and_get_balance(
        json_rpc_client,
        ft_balance_cache,
        &block_header.prev_hash,
        contract_id.clone(),
        &sender_id,
        &sender_delta_amount,
    )
    .await?;

    // I calc it just to check the logic. Should be dropped in the end
    // actually they could be different if the block contains more than one operations with this account
    // but it's enough to check at least something
    let correct_sender_value = ft_balance_utils::get_balance_from_rpc(
        json_rpc_client,
        &block_header.hash,
        contract_id.clone(),
        sender_id.as_str(),
    )
    .await?;
    assert_eq!(
        correct_sender_value,
        sender_absolute_amount.to_string().parse::<u128>()?
    );

    let receiver_delta_amount = BigDecimal::from_str(&ft_transfer_args.amount)?;
    let receiver_absolute_amount = ft_balance_utils::update_cache_and_get_balance(
        json_rpc_client,
        ft_balance_cache,
        &block_header.prev_hash,
        contract_id.clone(),
        &receiver_id,
        &receiver_delta_amount,
    )
    .await?;

    // I calc it just to check the logic. Should be dropped in the end
    // actually they could be different if the block contains more than one operations with this account
    // but it's enough to check at least something
    let correct_receiver_value = ft_balance_utils::get_balance_from_rpc(
        json_rpc_client,
        &block_header.hash,
        contract_id.clone(),
        receiver_id.as_str(),
    )
    .await?;
    assert_eq!(
        correct_receiver_value,
        receiver_absolute_amount.to_string().parse::<u128>()?
    );

    Ok(vec![
        CoinEvent {
            event_index: crate::db_adapters::compose_db_index(
                block_header.timestamp,
                shard_id,
                events::Event::WrapNear,
                starting_index,
            )?,
            receipt_id: outcome.receipt.receipt_id.to_string(),
            block_timestamp: BigDecimal::from(block_header.timestamp),
            contract_account_id: contract_id.to_string(),
            affected_account_id: sender_id.to_string(),
            involved_account_id: Some(receiver_id.to_string()),
            delta_amount: sender_delta_amount,
            absolute_amount: sender_absolute_amount,
            // standard: "".to_string(),
            // coin_id: "".to_string(),
            cause: "TRANSFER".to_string(),
            status: crate::db_adapters::get_status(&outcome.execution_outcome.outcome.status),
            event_memo: ft_transfer_args
                .memo
                .as_ref()
                .map(|s| s.escape_default().to_string()),
        },
        CoinEvent {
            event_index: crate::db_adapters::compose_db_index(
                block_header.timestamp,
                shard_id,
                events::Event::WrapNear,
                starting_index + 1,
            )?,
            receipt_id: outcome.receipt.receipt_id.to_string(),
            block_timestamp: BigDecimal::from(block_header.timestamp),
            contract_account_id: contract_id.to_string(),
            affected_account_id: receiver_id.to_string(),
            involved_account_id: Some(sender_id.to_string()),
            delta_amount: receiver_delta_amount,
            absolute_amount: receiver_absolute_amount,
            // standard: "".to_string(),
            // coin_id: "".to_string(),
            cause: "TRANSFER".to_string(),
            status: crate::db_adapters::get_status(&outcome.execution_outcome.outcome.status),
            event_memo: ft_transfer_args
                .memo
                .as_ref()
                .map(|s| s.escape_default().to_string()),
        },
    ])
}
