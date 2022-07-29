// TODO cleanup imports in all the files in the end
use cached::SizedCache;
use clap::Parser;
use dotenv::dotenv;
use futures::StreamExt;
use std::env;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use crate::configs::Opts;
use near_lake_framework::near_indexer_primitives;

mod configs;
mod db_adapters;
mod models;
mod rpc_helpers;

// Categories for logging
// TODO naming
pub(crate) const INDEXER: &str = "indexer";

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const MAX_DELAY_TIME: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct AccountWithContract {
    pub account_id: near_primitives::types::AccountId,
    pub contract_account_id: near_primitives::types::AccountId,
}

pub type FtBalanceCache =
    std::sync::Arc<Mutex<SizedCache<AccountWithContract, near_primitives::types::Balance>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    let opts: Opts = Opts::parse();

    // todo it's a good idea to profile it.
    //  From time to time, it works slow as hell, sometimes rpc is not responding at all. Probably it's UAE network jokes

    // let pool = sqlx::PgPool::connect_with(options).await?;
    let pool = sqlx::PgPool::connect(&env::var("DATABASE_URL")?).await?;
    // TODO Error: while executing migrations: error returned from database: 1128 (HY000): Function 'near_indexer.GET_LOCK' is not defined
    // sqlx::migrate!().run(&pool).await?;

    // let start_block_height = match opts.start_block_height {
    //     Some(x) => x,
    //     None => models::start_after_interruption(&pool).await?,
    // };

    // wrap near
    // ft_transfer 68814912
    // near_withdraw 68814918
    let start_block_height: u64 = 68814912; // 68814906 ; //60116646; //60116605; //53400020;

    let config = near_lake_framework::LakeConfigBuilder::default()
        .s3_bucket_name(opts.s3_bucket_name)
        .s3_region_name(opts.s3_region_name)
        .start_block_height(start_block_height)
        .blocks_preload_pool_size(100)
        .build()?;
    init_tracing();

    let (lake_handle, stream) = near_lake_framework::streamer(config);
    let json_rpc_client = near_jsonrpc_client::JsonRpcClient::connect(&opts.near_archival_rpc_url);

    // We want to prevent unnecessary RPC queries to find previous balance
    let ft_balance_cache: FtBalanceCache =
        std::sync::Arc::new(Mutex::new(SizedCache::with_size(100_000)));

    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(|streamer_message| {
            handle_streamer_message(streamer_message, &pool, &json_rpc_client, &ft_balance_cache)
        })
        .buffer_unordered(1usize);

    // let mut time_now = std::time::Instant::now();
    while let Some(handle_message) = handlers.next().await {
        match handle_message {
            Ok(_block_height) => {
                // let elapsed = time_now.elapsed();
                // println!(
                //     "Elapsed time spent on block {}: {:.3?}",
                //     block_height, elapsed
                // );
                // time_now = std::time::Instant::now();
            }
            Err(e) => {
                return Err(anyhow::anyhow!(e));
            }
        }
    }

    // propagate errors from the Lake Framework
    match lake_handle.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(anyhow::Error::from(e)), // JoinError
    }
}

async fn handle_streamer_message(
    streamer_message: near_indexer_primitives::StreamerMessage,
    pool: &sqlx::Pool<sqlx::Postgres>,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    ft_balance_cache: &FtBalanceCache,
) -> anyhow::Result<u64> {
    // if streamer_message.block.header.height % 100 == 0 {
    eprintln!(
        "{} / shards {}",
        streamer_message.block.header.height,
        streamer_message.shards.len()
    );
    // }

    db_adapters::events::store_events(pool, json_rpc_client, &streamer_message, ft_balance_cache)
        .await?;
    Ok(streamer_message.block.header.height)
}

fn init_tracing() {
    let mut env_filter = EnvFilter::new("near_lake_framework=info");

    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if !rust_log.is_empty() {
            for directive in rust_log.split(',').filter_map(|s| match s.parse() {
                Ok(directive) => Some(directive),
                Err(err) => {
                    eprintln!("Ignoring directive `{}`: {}", s, err);
                    None
                }
            }) {
                env_filter = env_filter.add_directive(directive);
            }
        }
    }

    tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();
}
