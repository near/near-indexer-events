# Indexer Events

Async Postgres-compatible solution to load the data from NEAR blockchain.
Based on [NEAR Lake Framework](https://github.com/near/near-lake-framework-rs).

See [Indexer Base](https://github.com/near/near-indexer-base#indexer-base) docs for all the explanations, installation guide, etc.

This solution collects balance-changing events about FTs, NFTs, etc.

### High-level overview of the logic

For each block, contract and affected account_id, we:
1. Find the previous balance (from RPC, asking for the balance at previous block, or from local cache);
2. Collect all balance-changing events;
3. Apply balance changes line-by-line, it gives us the resulting balance at the end of the block;
4. Ask RPC for the resulting balance at the end of the block, compare the results;
5. If the result is the same, we insert these events to the DB;
6. If the result differs, we drop these events, mark the contract as inconsistent, and stop collecting any activity about this contract.

And some more information that you may find useful:
- We can index the blockchain from any point of time. The code does not check if all the previous history is collected.
- Potentially, some events may be skipped. The absolute balance still remains correct.
- We can re-run infinite number of indexers writing at the same DB, they may index same or different parts of the blockchain. It should not break the flow. There is only one limitation: if we ban the contract at block X, the future data about this contract (blocks X+1 and further) should be dropped manually.
- We may drop all the data about inconsistent contract, add custom implementation and re-run indexer to collect the data related to this contract again.
- We can't give 100% guarantee on each event, because we can ask RPC for the balance only at the end of the block. Though, we do check the correctness at the every end of the block. So, we expect the data is correct.
- If the check (correctness of the balance) is failed, the data will not be saved, and the contract will be marked as inconsistent.


### Why existing `assets__*` tables are not enough?

`assets__non_fungible_token_events`, `assets__fungible_token_events` do not have the sorting column.
In the current solution, we've added artificial `event_index` column.

`assets__fungible_token_events` does not have the absolute value.  
The new table stores the data in the format of affected/involved account_id, and you can see the resulting balance.

### What else do I need to know?

This indexer uses RPC as the dependency for collecting information from the contracts.

Main purpose here is to support NEPs with events, but we also decided to support some legacy contracts.

### What if my contract does not produce events?

Please go and update your contract with our new [SDK](https://github.com/near/near-sdk-rs).

If it's important for you to collect all the previous history as well, you need to make the contribution and implement your own legacy handler.  
You can use [existing handlers](src/db_adapters/coin/legacy) as the example, [wrap_near](src/db_adapters/coin/legacy/wrap_near.rs) may be a good starting point.

### My contract produces events/there's a custom legacy logic for my contract, but the DB still ignores me. Why?

It means that we've found inconsistency in the data you provided with the data we've queried by RPC.  
To be more clear, we collected all the logs/performed all the legacy logic, we know all the changed balances for all the active users at the end of the block.
After that, we ask the RPC to provide all the needed balances.
The numbers should be the same.
If they are not the same, it means the data is inconsistent.

When we meet the first inconsistency, we mark such contract as "non-trusted" and we no longer collect the data from it.  
If you want to fix this, you need to write/edit [legacy handler](src/db_adapters/coin/legacy/DOC.md) for your contract.
