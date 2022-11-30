use std::env;

use clap::Parser;
use tracing_subscriber::EnvFilter;

/// NEAR Indexer for Explorer
/// Watches for stream of blocks from the chain
#[derive(Parser, Debug)]
#[clap(
    version,
    author,
    about,
    disable_help_subcommand(true),
    propagate_version(true),
    next_line_help(true)
)]
pub(crate) struct Opts {
    /// Enabled Indexer for Explorer debug level of logs
    #[clap(long)]
    pub debug: bool,
    /// AWS S3 bucket name to get the stream from
    #[clap(long, env)]
    pub s3_bucket_name: String,
    /// AWS Access Key with the rights to read from AWS S3
    #[clap(long, env)]
    pub lake_aws_access_key: String,
    #[clap(long, env)]
    /// AWS Secret Access Key with the rights to read from AWS S3
    pub lake_aws_secret_access_key: String,
    /// AWS S3 bucket region
    #[clap(long, env)]
    pub s3_region_name: String,
    /// Block height to start the stream from
    #[clap(long, short, env)]
    pub start_block_height: u64,
    #[clap(long, short, env)]
    pub near_archival_rpc_url: String,
    // Chain ID: testnet or mainnet
    #[clap(long, env)]
    pub chain_id: String,
    /// Port to enable metrics/health service
    #[clap(long, short, env)]
    pub port: u16,
}

pub(crate) fn init_tracing(debug: bool) -> anyhow::Result<()> {
    let mut env_filter = EnvFilter::new("indexer_events=info");

    if debug {
        env_filter = env_filter
            .add_directive("near_lake_framework=debug".parse()?);
    }

    if let Ok(rust_log) = env::var("RUST_LOG") {
        if !rust_log.is_empty() {
            for directive in rust_log.split(',').filter_map(|s| match s.parse() {
                Ok(directive) => Some(directive),
                Err(err) => {
                    tracing::warn!(
                        target: crate::LOGGING_PREFIX,
                        "Ignoring directive `{}`: {}",
                        s,
                        err
                    );
                    None
                }
            }) {
                env_filter = env_filter.add_directive(directive);
            }
        }
    }

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr);

    if std::env::var("ENABLE_JSON_LOGS").is_ok() {
        subscriber.json().init();
    } else {
        subscriber.compact().init();
    }

    Ok(())
}
