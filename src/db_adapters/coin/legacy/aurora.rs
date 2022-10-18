use crate::db_adapters;
use crate::db_adapters::coin::FT_LEGACY;
use crate::db_adapters::Event;
use crate::db_adapters::{coin, contracts};
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use near_primitives::borsh;
use near_primitives::borsh::{BorshDeserialize, BorshSerialize};
use near_primitives::types::AccountId;
use near_primitives::views::{ActionView, ExecutionStatusView, ReceiptEnumView};
use serde::Deserialize;
use std::io;
use std::ops::Mul;
use std::str::FromStr;

#[derive(Deserialize, Debug, Clone)]
struct FtTransfer {
    pub receiver_id: AccountId,
    pub amount: String,
    pub memo: Option<String>,
}

// Took from the link below + some places around
// https://github.com/aurora-is-near/aurora-engine/blob/master/engine-types/src/parameters.rs
/// withdraw NEAR eth-connector call args
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct WithdrawCallArgs {
    pub recipient_address: Address,
    pub amount: u128,
}

/// Base Eth Address type
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Address(primitive_types::H160);

impl BorshSerialize for Address {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(self.0.as_bytes())
    }
}

impl BorshDeserialize for Address {
    fn deserialize(buf: &mut &[u8]) -> io::Result<Self> {
        if buf.len() < 20 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "ETH_WRONG_ADDRESS_LENGTH",
            ));
        }
        // Guaranty no panics. The length checked early
        let address = Self(primitive_types::H160::from_slice(&buf[..20]));
        *buf = &buf[20..];
        Ok(address)
    }
}

pub(crate) async fn collect_aurora(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    ft_balance_cache: &crate::FtBalanceCache,
    contracts: &crate::ActiveContracts,
) -> anyhow::Result<Vec<CoinEvent>> {
    if contracts::check_contract_state(
        &AccountId::from_str("aurora")?,
        FT_LEGACY,
        block_header,
        contracts,
    )
    .await?
    {
        return Ok(vec![]);
    }
    let mut events: Vec<CoinEvent> = vec![];

    for outcome in receipt_execution_outcomes {
        if outcome.receipt.receiver_id != AccountId::from_str("aurora")?
            || !db_adapters::events::extract_events(outcome).is_empty()
        {
            continue;
        }
        if let ReceiptEnumView::Action { actions, .. } = &outcome.receipt.receipt {
            for action in actions {
                process_aurora_functions(
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
    Ok(events)
}

async fn process_aurora_functions(
    events: &mut Vec<CoinEvent>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    shard_id: &near_indexer_primitives::types::ShardId,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    cache: &crate::FtBalanceCache,
    action: &ActionView,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
) -> anyhow::Result<()> {
    let (method_name, args, ..) = match action {
        ActionView::FunctionCall {
            method_name,
            args,
            deposit,
            ..
        } => (method_name, args, deposit),
        _ => return Ok(()),
    };

    // Note: aurora (now always, but) usually has binary args
    let decoded_args = base64::decode(args)?;

    if vec![
        "new",
        "call",
        "new_eth_connector",
        "set_eth_connector_contract_data",
        "deposit",
        "submit",
        "deploy_erc20_token",
        "get_nep141_from_erc20",
        "ft_on_transfer",
    ]
    .contains(&method_name.as_str())
    {
        return Ok(());
    }

    // MINT may produce several events, where involved_account_id is always NULL
    // deposit do not mint anything; mint goes in finish_deposit
    if method_name == "finish_deposit" {
        for log in &outcome.execution_outcome.outcome.logs {
            lazy_static::lazy_static! {
                static ref RE: regex::Regex = regex::Regex::new(r"^Mint (?P<amount>(0|[1-9][0-9]*)) nETH tokens for: (?P<account_id>[a-z0-9\.\-]+)$").unwrap();
            }

            if let Some(cap) = RE.captures(log) {
                let amount = match cap.name("amount") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected mint log format in aurora: {}\n Expected format: Mint <amount> nETH tokens for: <account_id>", log)
                };
                if amount == "0" {
                    continue;
                }
                let account_id = match cap.name("account_id") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected mint log format in aurora: {}\n Expected format: Mint <amount> nETH tokens for: <account_id>", log)
                };

                let delta = BigDecimal::from_str(amount)?;
                let base = db_adapters::get_base(
                    Event::Aurora,
                    shard_id,
                    events.len(),
                    outcome,
                    block_header,
                )?;
                let custom = coin::FtEvent {
                    affected_id: AccountId::from_str(account_id)?,
                    involved_id: None,
                    delta,
                    cause: "MINT".to_string(),
                    memo: None,
                };
                events.push(
                    coin::build_event(json_rpc_client, cache, block_header, base, custom).await?,
                );
            };
        }

        return Ok(());
    }

    // TRANSFER produces 2 events
    // 1. affected_account_id is sender, delta is negative, absolute_amount decreased
    // 2. affected_account_id is receiver, delta is positive, absolute_amount increased
    if method_name == "ft_transfer" || method_name == "ft_transfer_call" {
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

        let base =
            db_adapters::get_base(Event::Aurora, shard_id, events.len(), outcome, block_header)?;
        let custom = coin::FtEvent {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: Some(ft_transfer_args.receiver_id.clone()),
            delta: negative_delta,
            cause: "TRANSFER".to_string(),
            memo: memo.clone(),
        };
        events.push(coin::build_event(json_rpc_client, cache, block_header, base, custom).await?);

        let base =
            db_adapters::get_base(Event::Aurora, shard_id, events.len(), outcome, block_header)?;
        let custom = coin::FtEvent {
            affected_id: ft_transfer_args.receiver_id,
            involved_id: Some(outcome.receipt.predecessor_id.clone()),
            delta,
            cause: "TRANSFER".to_string(),
            memo,
        };
        events.push(coin::build_event(json_rpc_client, cache, block_header, base, custom).await?);
        return Ok(());
    }

    // If TRANSFER failed, it could be revoked. The procedure is the same as for TRANSFER
    if method_name == "ft_resolve_transfer" {
        for log in &outcome.execution_outcome.outcome.logs {
            lazy_static::lazy_static! {
                static ref RE: regex::Regex = regex::Regex::new(r"^Refund amount (?P<amount>(0|[1-9][0-9]*)) from (?P<from_account_id>[a-z0-9\.\-]+) to (?P<to_account_id>[a-z0-9\.\-]+)$").unwrap();
            }

            if let Some(cap) = RE.captures(log) {
                let amount = match cap.name("amount") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected ft_resolve_transfer log format in aurora: {}\n Expected format: Refund amount <amount> from <account_id> to <account_id>", log)
                };
                if amount == "0" {
                    continue;
                }
                let from_account_id = match cap.name("from_account_id") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected ft_resolve_transfer log format in aurora: {}\n Expected format: Refund amount <amount> from <account_id> to <account_id>", log)
                };
                let to_account_id = match cap.name("to_account_id") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected ft_resolve_transfer log format in aurora: {}\n Expected format: Refund amount <amount> from <account_id> to <account_id>", log)
                };

                let delta = BigDecimal::from_str(amount)?;
                let negative_delta = delta.clone().mul(BigDecimal::from(-1));

                let base = db_adapters::get_base(
                    Event::Aurora,
                    shard_id,
                    events.len(),
                    outcome,
                    block_header,
                )?;
                let custom = coin::FtEvent {
                    affected_id: AccountId::from_str(from_account_id)?,
                    involved_id: Some(AccountId::from_str(to_account_id)?),
                    delta: negative_delta,
                    cause: "TRANSFER".to_string(),
                    memo: None,
                };
                events.push(
                    coin::build_event(json_rpc_client, cache, block_header, base, custom).await?,
                );

                let base = db_adapters::get_base(
                    Event::Aurora,
                    shard_id,
                    events.len(),
                    outcome,
                    block_header,
                )?;
                let custom = coin::FtEvent {
                    affected_id: AccountId::from_str(to_account_id)?,
                    involved_id: Some(AccountId::from_str(from_account_id)?),
                    delta,
                    cause: "TRANSFER".to_string(),
                    memo: None,
                };
                events.push(
                    coin::build_event(json_rpc_client, cache, block_header, base, custom).await?,
                );
            };
        }
        return Ok(());
    }

    if method_name == "withdraw" {
        let args = WithdrawCallArgs::try_from_slice(&decoded_args)?;
        let negative_delta =
            BigDecimal::from_str(&args.amount.to_string())?.mul(BigDecimal::from(-1));
        let base =
            db_adapters::get_base(Event::Aurora, shard_id, events.len(), outcome, block_header)?;
        let custom = coin::FtEvent {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: None,
            delta: negative_delta,
            cause: "BURN".to_string(),
            memo: None,
        };
        events.push(coin::build_event(json_rpc_client, cache, block_header, base, custom).await?);

        return Ok(());
    }

    tracing::error!(
        target: crate::LOGGING_PREFIX,
        "{} method {}, receipt {}",
        block_header.height,
        method_name,
        outcome.receipt.receipt_id
    );
    Ok(())
}
