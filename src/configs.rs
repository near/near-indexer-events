use std::env;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::LOGGING_PREFIX;

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

impl Opts {
    // Creates AWS Credentials for NEAR Lake
    fn lake_credentials(&self) -> aws_types::credentials::SharedCredentialsProvider {
        let provider = aws_types::Credentials::new(
            self.lake_aws_access_key.clone(),
            self.lake_aws_secret_access_key.clone(),
            None,
            None,
            "events_indexer",
        );
        aws_types::credentials::SharedCredentialsProvider::new(provider)
    }

    /// Creates AWS Shared Config for NEAR Lake
    pub fn lake_aws_sdk_config(&self) -> aws_types::sdk_config::SdkConfig {
        aws_types::sdk_config::SdkConfig::builder()
            .credentials_provider(self.lake_credentials())
            .region(aws_types::region::Region::new(self.s3_region_name.clone()))
            .build()
    }

    pub async fn get_lake_config(&self) -> near_lake_framework::LakeConfig {
        let s3_config = aws_sdk_s3::config::Builder::from(&self.lake_aws_sdk_config()).build();
        let config_builder = near_lake_framework::LakeConfigBuilder::default().s3_config(s3_config);

        tracing::info!(target: LOGGING_PREFIX, "Chain_id: {}", self.chain_id);

        match self.chain_id.as_str() {
            "mainnet" => config_builder
                .mainnet()
                .start_block_height(self.start_block_height),
            "testnet" => config_builder
                .testnet()
                .start_block_height(self.start_block_height),
            _ => panic!("CHAIN_ID is not set to a valid enviornment name. Try `mainnet` or `testnet`")
        }
        .build()
        .expect("Failed to build LakeConfig")
    }
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