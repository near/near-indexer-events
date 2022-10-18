use crate::models;
use crate::models::chunked_insert;
use bigdecimal::BigDecimal;
use cached::Cached;
use near_lake_framework::near_indexer_primitives;

pub(crate) async fn update_contracts(
    pool: &sqlx::Pool<sqlx::Postgres>,
    contracts: &crate::ActiveContracts,
) -> anyhow::Result<()> {
    let mut contracts_lock = contracts.lock().await;
    let contracts_for_db: Vec<models::contracts::Contract> = contracts_lock
        .iter_mut()
        .filter_map(|(_, contract)| {
            let res = if contract.should_add_to_db {
                Some(contract.clone())
            } else {
                None
            };
            contract.should_add_to_db = false;
            res
        })
        .collect();
    drop(contracts_lock);
    // we can use insert here, there's "on conflict do update" logic implemented
    chunked_insert(pool, &contracts_for_db).await?;
    Ok(())
}

// Updates contracts cache, returns true is the contract data is inconsistent
pub(crate) async fn check_contract_state(
    contract_account_id: &near_primitives::types::AccountId,
    standard: &str,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    contracts: &crate::ActiveContracts,
) -> anyhow::Result<bool> {
    let mut contracts_lock = contracts.lock().await;
    let is_banned = if let Some(x) = contracts_lock.cache_get(contract_account_id) {
        x.inconsistency_found_at_timestamp.is_some()
    } else {
        contracts_lock.insert(
            contract_account_id.clone(),
            models::contracts::Contract {
                contract_account_id: contract_account_id.to_string(),
                standard: standard.to_string(),
                first_event_at_timestamp: BigDecimal::from(block_header.timestamp),
                first_event_at_block_height: BigDecimal::from(block_header.height),
                inconsistency_found_at_timestamp: None,
                inconsistency_found_at_block_height: None,
                should_add_to_db: true,
            },
        );
        false
    };
    drop(contracts_lock);
    Ok(is_banned)
}

pub(crate) async fn mark_contract_inconsistent(
    pool: &sqlx::Pool<sqlx::Postgres>,
    contract_account_id: &near_primitives::types::AccountId,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    contracts: &crate::ActiveContracts,
) -> anyhow::Result<()> {
    let mut contracts_lock = contracts.lock().await;
    if let Some(x) = contracts_lock.cache_get_mut(contract_account_id) {
        x.inconsistency_found_at_timestamp = Some(BigDecimal::from(block_header.timestamp));
        x.inconsistency_found_at_block_height = Some(BigDecimal::from(block_header.height));
    }
    drop(contracts_lock);

    // it's enough to init here only fields 1, 5, 6, as they are involved in UPDATE
    let contract = models::contracts::Contract {
        contract_account_id: contract_account_id.to_string(),
        standard: "".to_string(),
        first_event_at_timestamp: Default::default(),
        first_event_at_block_height: Default::default(),
        inconsistency_found_at_timestamp: Some(BigDecimal::from(block_header.timestamp)),
        inconsistency_found_at_block_height: Some(BigDecimal::from(block_header.height)),
        should_add_to_db: false,
    };
    let query = r"UPDATE contracts
                            SET inconsistency_found_at_timestamp = $5, inconsistency_found_at_block_height = $6
                            WHERE account_id = $1 AND inconsistency_found_at_timestamp IS NULL";
    models::update_retry_or_panic(pool, query, &contract, 10).await
}
