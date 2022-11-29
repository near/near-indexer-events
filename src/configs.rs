use clap::Parser;

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
    pub http_port: u16,
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
}
