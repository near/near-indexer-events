use crate::db_adapters::{events, ft_balance_utils};
use crate::models;
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use near_primitives::types::AccountId;
use near_primitives::views::{ActionView, ExecutionStatusView, ReceiptEnumView};
use serde::Deserialize;
use std::ops::Mul;
use std::str::FromStr;

#[derive(Deserialize, Debug, Clone)]
struct FtTransfer {
    pub receiver_id: AccountId,
    pub amount: String,
    pub memo: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct FtRefund {
    pub receiver_id: AccountId,
    pub sender_id: AccountId,
    pub amount: String,
    pub memo: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct NearWithdraw {
    pub amount: String,
}

struct WrapNearBase {
    pub event_index: BigDecimal,
    pub receipt_id: String,
    pub block_timestamp: BigDecimal,
    pub contract_account_id: AccountId,
    pub status: ExecutionStatusView,
}

struct WrapNearCustom {
    pub affected_id: AccountId,
    pub involved_id: Option<AccountId>,
    pub delta: BigDecimal,
    pub cause: String,
    pub memo: Option<String>,
}

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
                process_wrap_near_functions(
                    &mut events,
                    json_rpc_client,
                    shard_id,
                    block_header,
                    ft_balance_cache,
                    action,
                    outcome,
                )
                    .await?;
            }
        }
    }

    models::chunked_insert(pool, &events).await?;
    Ok(())
}

async fn process_wrap_near_functions(
    events: &mut Vec<CoinEvent>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    shard_id: &near_indexer_primitives::types::ShardId,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    cache: &crate::FtBalanceCache,
    action: &ActionView,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
) -> anyhow::Result<()> {
    let (method_name, args, deposit) = match action {
        ActionView::FunctionCall {
            method_name,
            args,
            deposit,
            ..
        } => (method_name, args, deposit),
        _ => return Ok(()),
    };

    let decoded_args = base64::decode(args)?;

    if method_name == "storage_deposit" {
        return Ok(());
    }

    // MINT produces 1 event, where involved_account_id is NULL
    if method_name == "near_deposit" {
        // block 68814971, receipt 2g7EpDnSBNAv9iLeQX31DvJ3RedPteTPSoWT5PvdSbA6
        // https://explorer.near.org/transactions/A8VxiibuqVwvK6KqKkSqfRCsFVzw2Fe53Cyh744bMbYE#2g7EpDnSBNAv9iLeQX31DvJ3RedPteTPSoWT5PvdSbA6
        let delta = BigDecimal::from_str(&deposit.to_string())?;
        let base = get_base(shard_id, events.len(), outcome, block_header)?;
        let custom = WrapNearCustom {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: None,
            delta,
            cause: "MINT".to_string(),
            memo: None,
        };
        events.push(build_event(json_rpc_client, cache, block_header, base, custom).await?);
        return Ok(());
    }

    // TRANSFER produces 2 events
    // 1. affected_account_id is sender, delta is negative, absolute_amount decreased
    // 2. affected_account_id is receiver, delta is positive, absolute_amount increased
    if method_name == "ft_transfer" || method_name == "ft_transfer_call" {
        // ft_transfer
        // block 68814912, receipt 3npphttR2J4EmxMrB8NGSu3icjEhaqiWS5vP5FVTTfjJ
        // https://explorer.near.org/transactions/EXuc2SVXfvexPxNggo5XdQD9JKABfv9i7LwdUUTX4ySG#3npphttR2J4EmxMrB8NGSu3icjEhaqiWS5vP5FVTTfjJ

        // ft_transfer_call
        // block 68814941, receipt Huxfk2oVoPeWWKHgRgMY5VdJ1Cjec3HKGFhrRYMdrn2h
        // https://explorer.near.org/transactions/zVVciak15t3UcqjqmJzgvYVfPCMYEmbthQFnCeHzEtP#Huxfk2oVoPeWWKHgRgMY5VdJ1Cjec3HKGFhrRYMdrn2h
        let ft_transfer_args = match serde_json::from_slice::<FtTransfer>(&decoded_args) {
            Ok(x) => x,
            Err(err) => {
                match outcome.execution_outcome.outcome.status {
                    // We couldn't parse args for failed receipt. Let's just ignore it, we can't save it properly
                    ExecutionStatusView::Unknown | ExecutionStatusView::Failure(_) => return Ok(()),
                    ExecutionStatusView::SuccessValue(_)
                    | ExecutionStatusView::SuccessReceiptId(_) => {
                        anyhow::bail!(err)
                    }
                }
            }
        };

        let delta = BigDecimal::from_str(&ft_transfer_args.amount)?;
        let negative_delta = delta.clone().mul(BigDecimal::from(-1));
        let memo = ft_transfer_args
            .memo
            .as_ref()
            .map(|s| s.escape_default().to_string());

        let base = get_base(shard_id, events.len(), outcome, block_header)?;
        let custom = WrapNearCustom {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: Some(ft_transfer_args.receiver_id.clone()),
            delta: negative_delta,
            cause: "TRANSFER".to_string(),
            memo: memo.clone(),
        };
        events.push(build_event(json_rpc_client, cache, block_header, base, custom).await?);

        let base = get_base(shard_id, events.len(), outcome, block_header)?;
        let custom = WrapNearCustom {
            affected_id: ft_transfer_args.receiver_id,
            involved_id: Some(outcome.receipt.predecessor_id.clone()),
            delta,
            cause: "TRANSFER".to_string(),
            memo,
        };
        events.push(build_event(json_rpc_client, cache, block_header, base, custom).await?);
        return Ok(());
    }

    // If TRANSFER failed, it could be revoked. The procedure is the same as for TRANSFER
    if method_name == "ft_resolve_transfer" {
        if outcome.execution_outcome.outcome.logs.is_empty() {
            // ft_transfer_call was successful, there's nothing to return back
            return Ok(());
        }
        let ft_refund_args = match serde_json::from_slice::<FtRefund>(&decoded_args) {
            Ok(x) => x,
            Err(err) => {
                match outcome.execution_outcome.outcome.status {
                    // We couldn't parse args for failed receipt. Let's just ignore it, we can't save it properly
                    ExecutionStatusView::Unknown | ExecutionStatusView::Failure(_) => return Ok(()),
                    ExecutionStatusView::SuccessValue(_)
                    | ExecutionStatusView::SuccessReceiptId(_) => {
                        anyhow::bail!(err)
                    }
                }
            }
        };
        let delta = BigDecimal::from_str(&ft_refund_args.amount)?;
        let negative_delta = delta.clone().mul(BigDecimal::from(-1));
        let memo = ft_refund_args
            .memo
            .as_ref()
            .map(|s| s.escape_default().to_string());

        for log in &outcome.execution_outcome.outcome.logs {
            if log == "The account of the sender was deleted" {
                // I never met this case so it's better to re-check it manually when we find it
                tracing::error!(
                    target: crate::INDEXER,
                    "The account of the sender was deleted {}",
                    block_header.height
                );

                // we should revert ft_transfer_call, but there's no receiver_id. We should burn tokens
                let base = get_base(shard_id, events.len(), outcome, block_header)?;
                let custom = WrapNearCustom {
                    affected_id: ft_refund_args.receiver_id,
                    involved_id: None,
                    delta: negative_delta,
                    cause: "BURN".to_string(),
                    memo,
                };
                events.push(build_event(json_rpc_client, cache, block_header, base, custom).await?);
                return Ok(());
            }
            if log.starts_with("Refund ") {
                // we should revert ft_transfer_call
                let base = get_base(shard_id, events.len(), outcome, block_header)?;
                let custom = WrapNearCustom {
                    affected_id: ft_refund_args.receiver_id.clone(),
                    involved_id: Some(ft_refund_args.sender_id.clone()),
                    delta: negative_delta,
                    cause: "TRANSFER".to_string(),
                    memo: memo.clone(),
                };
                events.push(build_event(json_rpc_client, cache, block_header, base, custom).await?);

                let base = get_base(shard_id, events.len(), outcome, block_header)?;
                let custom = WrapNearCustom {
                    affected_id: ft_refund_args.sender_id,
                    involved_id: Some(ft_refund_args.receiver_id),
                    delta,
                    cause: "TRANSFER".to_string(),
                    memo,
                };
                events.push(build_event(json_rpc_client, cache, block_header, base, custom).await?);
                return Ok(());
            }
        }
        return Ok(());
    }

    // BURN produces 1 event, where involved_account_id is NULL
    if method_name == "near_withdraw" {
        // block 68814918, receipt 9nPRqp6vtr7iEDoDnPsinY34msJTTh8GUw4BnYQcW8Gu
        // https://explorer.near.org/transactions/4LP9CZwWZ75vdUyLnnS33TqJCDBg5drwsThm7PCHosWZ#9nPRqp6vtr7iEDoDnPsinY34msJTTh8GUw4BnYQcW8Gu
        let ft_burn_args = match serde_json::from_slice::<NearWithdraw>(&decoded_args) {
            Ok(x) => x,
            Err(err) => {
                match outcome.execution_outcome.outcome.status {
                    // We couldn't parse args for failed receipt. Let's just ignore it, we can't save it properly
                    ExecutionStatusView::Unknown | ExecutionStatusView::Failure(_) => return Ok(()),
                    ExecutionStatusView::SuccessValue(_)
                    | ExecutionStatusView::SuccessReceiptId(_) => {
                        anyhow::bail!(err)
                    }
                }
            }
        };
        let negative_delta = BigDecimal::from_str(&ft_burn_args.amount)?.mul(BigDecimal::from(-1));

        let base = get_base(shard_id, events.len(), outcome, block_header)?;
        let custom = WrapNearCustom {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: None,
            delta: negative_delta,
            cause: "BURN".to_string(),
            memo: None,
        };
        events.push(build_event(json_rpc_client, cache, block_header, base, custom).await?);
        return Ok(());
    }

    tracing::error!(
        target: crate::INDEXER,
        "{} method {}",
        block_header.height,
        method_name
    );
    Ok(())
}

fn get_base(
    shard_id: &near_indexer_primitives::types::ShardId,
    starting_index: usize,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<WrapNearBase> {
    Ok(WrapNearBase {
        event_index: crate::db_adapters::compose_db_index(
            block_header.timestamp,
            shard_id,
            events::Event::WrapNear,
            starting_index,
        )?,
        receipt_id: outcome.receipt.receipt_id.to_string(),
        block_timestamp: BigDecimal::from(block_header.timestamp),
        contract_account_id: AccountId::from_str("wrap.near")?,
        status: outcome.execution_outcome.outcome.status.clone(),
    })
}

async fn build_event(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    ft_balance_cache: &crate::FtBalanceCache,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    base: WrapNearBase,
    custom: WrapNearCustom,
) -> anyhow::Result<CoinEvent> {
    let absolute_amount = ft_balance_utils::update_cache_and_get_balance(
        json_rpc_client,
        ft_balance_cache,
        &base.status,
        &block_header.prev_hash,
        base.contract_account_id.clone(),
        &custom.affected_id,
        &custom.delta,
    )
        .await?;

    // I calc it just to check the logic. Should be dropped in the end
    // actually they could be different if the block contains more than one operations with this account
    // but it's enough to check at least something
    let correct_value = ft_balance_utils::get_balance_from_rpc(
        json_rpc_client,
        &block_header.hash,
        base.contract_account_id.clone(),
        custom.affected_id.as_str(),
    )
        .await?;
    if correct_value != absolute_amount.to_string().parse::<u128>()? {
        tracing::error!(
            target: crate::INDEXER,
            "{} different amounts for {}: expected {}, actual {}. Index {}",
            block_header.height,
            custom.affected_id.as_str(),
            correct_value,
            absolute_amount.to_string().parse::<u128>()?,
            base.event_index,
        );
    }

    Ok(CoinEvent {
        event_index: base.event_index,
        receipt_id: base.receipt_id,
        block_timestamp: base.block_timestamp,
        contract_account_id: base.contract_account_id.to_string(),
        affected_account_id: custom.affected_id.to_string(),
        involved_account_id: custom.involved_id.map(|id| id.to_string()),
        delta_amount: custom.delta,
        absolute_amount,
        // standard: "".to_string(),
        // coin_id: "".to_string(),
        cause: custom.cause,
        status: crate::db_adapters::get_status(&base.status),
        event_memo: custom.memo,
    })
}
