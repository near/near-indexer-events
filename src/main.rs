// TODO cleanup imports in all the files in the end
use crate::configs::{init_tracing, Opts};
use clap::Parser;
use dotenv::dotenv;
use futures::StreamExt;
use near_lake_framework::near_indexer_primitives;
use std::env;
mod configs;
mod db_adapters;
mod metrics;
mod models;

#[macro_use]
extern crate lazy_static;

pub(crate) const LOGGING_PREFIX: &str = "indexer_events";

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const MAX_DELAY_TIME: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct AccountWithContract {
    pub account_id: near_primitives::types::AccountId,
    pub contract_account_id: near_primitives::types::AccountId,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    let opts: Opts = Opts::parse();

    let pool = sqlx::PgPool::connect(&env::var("DATABASE_URL")?).await?;

    let _worker_guard = init_tracing(opts.debug)?;

    let config: near_lake_framework::LakeConfig = opts.to_lake_config().await;
    let (_lake_handle, stream) = near_lake_framework::streamer(config);

    tokio::spawn(async move {
        let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
            .map(|streamer_message| handle_streamer_message(streamer_message, &pool))
            .buffer_unordered(1usize);

        while let Some(_handle_message) = handlers.next().await {}
    });

    metrics::init_metrics_server().await?;

    Ok(())
}

async fn handle_streamer_message(
    streamer_message: near_indexer_primitives::StreamerMessage,
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    metrics::BLOCK_PROCESSED_TOTAL.inc();
    // Prometheus Guage Metric type only supports i64 and f64 types for the time being, so we need to cast the type into i64
    metrics::LATEST_BLOCK_HEIGHT.set(i64::try_from(streamer_message.block.header.height)?);

    if streamer_message.block.header.height % 100 == 0 {
        tracing::info!(
            target: crate::LOGGING_PREFIX,
            "{} / shards {}",
            streamer_message.block.header.height,
            streamer_message.shards.len()
        );
    }

    db_adapters::events::store_events(pool, &streamer_message).await?;

    Ok(streamer_message.block.header.height)
}
