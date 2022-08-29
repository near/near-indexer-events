# Indexer Balances

Async Postgres-compatible solution to load the data from NEAR blockchain.
Based on [NEAR Lake Framework](https://github.com/near/near-lake-framework-rs).

See [Indexer Base](https://github.com/near/near-indexer-base#indexer-base) docs for all the explanations, installation guide, etc.

### Why `assets__*` tables are not enough?

`assets__non_fungible_token_events`, `assets__fungible_token_events` do not have the sorting column.
In the current solution, we've added artificial `index` column.

`assets__fungible_token_events` does not have the absolute value.  
The new table stores the data in the format of affected/involved account_id, and you can query the resulting balance.

### What else do I need to know?

This indexer uses RPC as the dependency for collecting information from the contracts.

Main purpose here is to support NEPs with events, but we also decided to support some legacy contracts.

### What if my contract does not produce events?

Please go and update your contract with our new [SDK](https://github.com/near/near-sdk-rs).

If it's important for you to collect all the previous history as well, you need to make the contribution and implement your own legacy handler.  
You can use [existing handlers](src/db_adapters/coin/legacy) as the example, [wrap_near](src/db_adapters/coin/legacy/wrap_near.rs) may be a good starting point.

### My contract produces events/there's a custom legacy logic for my contract, but the DB still ignores me. Why?

WIP https://github.com/near/near-indexer-events/issues/7

It means that we've found inconsistency in the data you provided with the data we've queried by RPC.  
To be more clear, we collected all the logs/performed all the legacy logic, we know all the changed balances for all the active users at the end of the block.
After that, we ask the RPC to provide all the needed balances.
The numbers should be the same.
If they are not the same, it means the data is inconsistent.

When we meet the first inconsistency, we mark such contract as "non-trusted" and we no longer collect the data from it.  
If you want to fix this, you need to write/edit your legacy handler for your contract.
