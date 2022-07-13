use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::FieldCount;

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct NonFungibleTokenEvent {
    pub emitted_for_receipt_id: String,
    pub emitted_at_block_timestamp: BigDecimal,
    pub emitted_in_shard_id: BigDecimal,
    pub emitted_index_of_event_entry_in_shard: i32,
    pub emitted_by_contract_account_id: String,
    pub token_id: String,
    pub event_kind: String,
    pub token_old_owner_account_id: String,
    pub token_new_owner_account_id: String,
    pub token_authorized_account_id: String,
    pub event_memo: String,
}

impl crate::models::SqlMethods for NonFungibleTokenEvent {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.emitted_for_receipt_id);
        args.add(&self.emitted_at_block_timestamp);
        args.add(&self.emitted_in_shard_id);
        args.add(&self.emitted_index_of_event_entry_in_shard);
        args.add(&self.emitted_by_contract_account_id);
        args.add(&self.token_id);
        args.add(&self.event_kind);
        args.add(&self.token_old_owner_account_id);
        args.add(&self.token_new_owner_account_id);
        args.add(&self.token_authorized_account_id);
        args.add(&self.event_memo);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO non_fungible_token_events VALUES ".to_owned()
            + &crate::models::create_placeholders(
                items_count,
                NonFungibleTokenEvent::field_count(),
            )?
            + " ON CONFLICT DO NOTHING")
    }

    fn name() -> String {
        "non_fungible_token_events".to_string()
    }
}
