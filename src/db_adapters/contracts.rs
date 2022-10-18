use crate::models;
use crate::models::chunked_insert;
use near_primitives::types::AccountId;
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use tokio::sync::Mutex;

pub(crate) struct ContractsHelper {
    contracts_for_db: std::sync::Arc<Mutex<HashMap<AccountId, models::contracts::Contract>>>,
    inconsistent_contracts: std::sync::Arc<Mutex<HashSet<AccountId>>>,
}

impl ContractsHelper {
    pub(crate) async fn restore_from_db(
        pool: &sqlx::Pool<sqlx::Postgres>,
        block_height: u64,
    ) -> anyhow::Result<ContractsHelper> {
        let query = r"SELECT contract_account_id FROM contracts
                  WHERE first_event_at_block_height <= $1::numeric(20, 0) AND inconsistency_found_at_block_height <= $1::numeric(20, 0)";
        let contracts_from_db =
            models::select_retry_or_panic(pool, query, &[block_height.to_string()], 10).await?;
        let inconsistent_contracts: HashSet<AccountId> = contracts_from_db
            .iter()
            .map(|value| value.get(0))
            .map(|account_id: String| {
                AccountId::from_str(&account_id)
                    .expect("Valid AccountId expected from contracts table")
            })
            .collect();
        Ok(ContractsHelper {
            contracts_for_db: std::sync::Arc::new(Mutex::new(HashMap::new())),
            inconsistent_contracts: std::sync::Arc::new(Mutex::new(inconsistent_contracts)),
        })
    }

    pub(crate) async fn is_contract_inconsistent(&self, account_id: &AccountId) -> bool {
        let contracts_lock = self.inconsistent_contracts.lock().await;
        let res = contracts_lock.contains(account_id);
        drop(contracts_lock);
        res
    }

    pub(crate) async fn mark_contract_inconsistent(
        &self,
        contract: models::contracts::Contract,
    ) -> anyhow::Result<()> {
        let mut cache_lock = self.inconsistent_contracts.lock().await;
        cache_lock.insert(AccountId::from_str(&contract.contract_account_id)?);
        drop(cache_lock);

        self.try_register_contract(contract).await
    }

    // we can speed up the solution by storing cache with known contracts
    pub(crate) async fn try_register_contract(
        &self,
        contract: models::contracts::Contract,
    ) -> anyhow::Result<()> {
        let mut db_lock = self.contracts_for_db.lock().await;
        db_lock.insert(
            AccountId::from_str(&contract.contract_account_id)?,
            contract,
        );
        drop(db_lock);
        Ok(())
    }

    pub(crate) async fn update_db(&self, pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<()> {
        let mut contracts_lock = self.contracts_for_db.lock().await;
        let contracts: Vec<models::contracts::Contract> = contracts_lock
            .drain()
            .map(|(_, contract)| contract)
            .collect();
        drop(contracts_lock);
        chunked_insert(pool, &contracts).await?;
        Ok(())
    }
}
