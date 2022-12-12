# Indexer Events

Async Postgres-compatible solution to load the data from NEAR blockchain.
Based on [NEAR Lake Framework](https://github.com/near/near-lake-framework-rs).

See [Indexer Base](https://github.com/near/near-indexer-base#indexer-base) docs for all the explanations, installation guide, etc.

This solution collects balance-changing events about FTs, NFTs, etc.

- We can index the blockchain from any point of time. The code does not check if all the previous history is collected.
- Potentially, some events may be skipped.
- We do not check the correctness of collected events, it should be done separately.
- We can re-run infinite number of indexers writing at the same DB, they may index same or different parts of the blockchain. It should not break the flow.

## Launching the indexer

There are a few configuration options that this micro-indexer supports. There are a few such variables that must be passed in as Environmental variables in a `.env` file and other configuration options that can either be provided as command line arguments or by environmental variables in an `.env`

### Environment Variables

| Environmental Variable             | Description                   | supported by CLI or `.env`                       | Required                   |
| ----------------- | ------------------------------------------------------------------ | --------------------------------------------- |  -------------------------------- |
| `DATABASE_URL`  | Specifies the Postgres Database where the data will be written to. |  `.env` only  | Required |
| `CHAIN_ID` | `mainnet` or `testnet` |  Both | Required |
| `START_BLOCK_HEIGHT` | Specifies the block height at which the indexer will start indexing from. On an absolute fresh launch, this variable needs to be specified. To index the complete history of the blockchain, start this process from the [genesis block](https://explorer.near.org/stats) of the specified network. |  Both |  Required on fresh start|
| `NEAR_ARCHIVAL_RPC_URL` | NEAR RPC Client | Both |  Required |
| `AWS_ACCESS_KEY_ID` | AWS credentials to access stream of data from Near Lake. If this ENV variable is not specified in `.env`, it will try to find it in `~/.aws/credentials` | `.env` or from `~/.aws/crednetials` | required |
| `AWS_SECRET_ACCESS_KEY` | AWS credentials to access stream of data from Near Lake. If this ENV variable is not specified in `.env`, it will try to find it in `~/.aws/credentials` | `.env` or from `~/.aws/crednetials` | required |
| `DEBUG`  | Enables Debug level logs | both | optional |
| `PORT` | Specifies the port where the metrics server will run. | both | optional |

After specifying all the required `REQUIRED` Environmental variables in an `.env` file, you may run the indexer 
```shell
cargo run
```
or override configuration options by passing them along side `cargo run` like such. 
```shell
cargo run  --  --start-block-height 9820210 --near-archival-rpc-url https://archival-rpc.mainnet.near.org --port 3000

```
## A brief note on Metrics Collection
To track down issues related to performance of indexer or to check in on what block the indexer has indexed till, we expose two observable metrics from within this application. 
- Block Processing Rate which shows the speed of proessing blocks. 
- Last block seen by the indexer which shows us what block the indexer has indexed till. 

### Why existing `assets__*` tables are not enough?

`assets__non_fungible_token_events`, `assets__fungible_token_events` do not have the sorting column.
In the current solution, we've added artificial `event_index` column.

The new `coin_events` table stores the data in the format of affected/involved account_id, that simplifies filtering by affected `account_id`.  
`coin_events` still does not have `absolute_value` column, so you have to collect it from RPC if needed.

### What if my contract does not produce events?

Please go and update your contract with our new [SDK](https://github.com/near/near-sdk-rs).

If it's important for you to collect all the previous history as well, you need to make the contribution and implement your own legacy handler.  
You can use [existing handlers](src/db_adapters/coin/legacy) as the example, [wrap_near](src/db_adapters/coin/legacy/wrap_near.rs) may be a good starting point.

### My contract produces events/there's a custom legacy logic for my contract, but the Enhanced API still ignores me. Why?

It means that we've found inconsistency in the data you provided with the data we've queried by RPC.  
To be more clear, we collected all the logs/performed all the legacy logic, we know all the changed balances for all the active users at the end of the block.
After that, we ask the RPC to provide all the needed balances.
The numbers should be the same.
If they are not the same, it means the data is inconsistent.

When we meet the inconsistency, we mark such contract as "non-trusted".  
If you want to fix this, you need to write/edit [legacy handler](src/db_adapters/coin/legacy/DOC.md) for your contract.

### Contribution Guide

Please refer to this [guide](https://github.com/near/near-indexer-for-explorer/blob/master/CONTRIBUTING.md) before submitting PRs to this repo 