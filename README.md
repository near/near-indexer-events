This is a tool to move the data from S3 to Postgres-compatible DB in a structured way.

## Migration

```bash
# Add the new migration
sqlx migrate add migration_name

# Apply migrations
sqlx migrate run
```

## Linux

```bash
sudo apt install git build-essential pkg-config libssl-dev tmux postgresql-client libpq-dev -y
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
cargo install --version=0.5.13 sqlx-cli --features postgres
ulimit -n 30000
cargo build --release
#!!! here you need to create .env in the root of the project, and .aws in ~
cargo run --release -- --s3-bucket-name near-lake-data-mainnet --s3-region-name eu-central-1 --near-archival-rpc-url https://archival-rpc.mainnet.near.org --start-block-height 9820210
```
