use bigdecimal::BigDecimal;
use cached::Cached;
use near_lake_framework::near_indexer_primitives;
use near_primitives::views::ExecutionStatusView;
use num_traits::Signed;
use std::ops::Add;
use std::str::FromStr;

pub(crate) async fn update_cache_and_get_balance(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    ft_balance_cache: &crate::FtBalanceCache,
    execution_status: &ExecutionStatusView,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    contract_id: near_primitives::types::AccountId,
    account_id: &str,
    delta_amount: &BigDecimal,
) -> anyhow::Result<BigDecimal> {
    let account_with_contract = crate::AccountWithContract {
        account_id: near_primitives::types::AccountId::from_str(account_id)?,
        contract_account_id: contract_id.clone(),
    };
    let prev_absolute_amount = BigDecimal::from_str(
        &get_balance(
            account_with_contract.clone(),
            &block_header.prev_hash,
            ft_balance_cache,
            json_rpc_client,
        )
        .await?
        .to_string(),
    )?;
    let absolute_amount = match execution_status {
        ExecutionStatusView::Unknown | ExecutionStatusView::Failure(_) => prev_absolute_amount,
        ExecutionStatusView::SuccessValue(_) | ExecutionStatusView::SuccessReceiptId(_) => {
            prev_absolute_amount.add(delta_amount)
        }
    };

    if absolute_amount.is_negative() {
        anyhow::bail!(
            "Balance {} is negative for account {}, contract {}, block {} {}. Delta {}",
            account_id,
            absolute_amount,
            contract_id,
            block_header.height,
            block_header.hash,
            delta_amount
        )
    }
    save_latest_balance(
        account_with_contract,
        absolute_amount.to_string().parse::<u128>()?,
        ft_balance_cache,
    )
    .await;
    Ok(absolute_amount)
}

async fn save_latest_balance(
    account_with_contract: crate::AccountWithContract,
    balance: near_indexer_primitives::types::Balance,
    ft_balance_cache: &crate::FtBalanceCache,
) {
    let mut balances_cache_lock = ft_balance_cache.lock().await;
    balances_cache_lock.cache_set(account_with_contract, balance);
    drop(balances_cache_lock);
}

pub(crate) async fn get_balance(
    account_with_contract: crate::AccountWithContract,
    block_hash: &near_indexer_primitives::CryptoHash,
    ft_balance_cache: &crate::FtBalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<near_primitives::types::Balance> {
    if let Some(balance) = get_balance_from_cache(&account_with_contract, ft_balance_cache).await {
        return Ok(balance);
    };
    get_balance_from_rpc_retriable(
        json_rpc_client,
        block_hash,
        account_with_contract.contract_account_id,
        account_with_contract.account_id.as_str(),
    )
    .await
}

pub(crate) async fn get_balance_from_rpc_retriable(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    block_hash: &near_indexer_primitives::CryptoHash,
    contract_id: near_primitives::types::AccountId,
    account_id: &str,
) -> anyhow::Result<near_primitives::types::Balance> {
    let mut interval = crate::INTERVAL;
    let mut retry_attempt = 0usize;

    loop {
        if retry_attempt == crate::db_adapters::RETRY_COUNT {
            anyhow::bail!(
                "Failed to perform query to RPC after {} attempts. Stop trying.\nContract {}, block_hash {}",
                crate::db_adapters::RETRY_COUNT,
                contract_id,
                block_hash.to_string()
            );
        }
        retry_attempt += 1;

        match get_balance_from_rpc(json_rpc_client, block_hash, contract_id.clone(), account_id)
            .await
        {
            Ok(res) => return Ok(res),
            Err(err) => {
                tracing::error!(
                    target: crate::INDEXER,
                    "Failed to request ft_balance_of from RPC for account {}, contract {}, block_hash {}.{}\n Retrying in {} milliseconds...",
                    account_id,
                    contract_id,
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

async fn get_balance_from_cache(
    account_with_contract: &crate::AccountWithContract,
    ft_balance_cache: &crate::FtBalanceCache,
) -> Option<near_primitives::types::Balance> {
    let mut balances_cache_lock = ft_balance_cache.lock().await;
    let result = balances_cache_lock
        .cache_get(account_with_contract)
        .copied();
    drop(balances_cache_lock);
    result
}

async fn get_balance_from_rpc(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    block_hash: &near_indexer_primitives::CryptoHash,
    contract_id: near_primitives::types::AccountId,
    account_id: &str,
) -> anyhow::Result<near_primitives::types::Balance> {
    let request = crate::rpc_helpers::get_function_call_request(
        block_hash,
        contract_id.clone(),
        "ft_balance_of",
        serde_json::json!({ "account_id": account_id }),
    );
    let response =
        crate::rpc_helpers::wrapped_call(json_rpc_client, request, block_hash, &contract_id)
            .await?;
    match serde_json::from_slice::<String>(&response.result) {
        Ok(x) => Ok(x.parse::<u128>()?),
        Err(_) => Ok(serde_json::from_slice::<u128>(&response.result)?),
    }
}

pub(crate) async fn check_balance(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    block_header: &near_primitives::views::BlockHeaderView,
    contract_id: &str,
    account_id: &str,
    amount: &BigDecimal,
) -> anyhow::Result<()> {
    let correct_value = get_balance_from_rpc_retriable(
        json_rpc_client,
        &block_header.hash,
        near_primitives::types::AccountId::from_str(contract_id)?,
        account_id,
    )
    .await?;
    if correct_value != amount.to_string().parse::<u128>()? {
        anyhow::bail!(
            "Balance is wrong for account {}, contract {}: expected {}, actual {}. Block {} {}",
            account_id,
            contract_id,
            correct_value,
            amount,
            block_header.height,
            block_header.hash,
        )
    }
    Ok(())
}
