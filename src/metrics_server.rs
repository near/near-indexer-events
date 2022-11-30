use actix_web::{get, App, HttpServer, Responder};
use prometheus::{Encoder, Gauge, IntCounter, IntGauge, Opts};

use crate::LOGGING_PREFIX;

pub type Result<T, E> = std::result::Result<T, E>;

pub fn try_create_int_counter(name: &str, help: &str) -> Result<IntCounter, prometheus::Error> {
    let opts = Opts::new(name, help);
    let counter = IntCounter::with_opts(opts)?;
    prometheus::register(Box::new(counter.clone()))?;
    Ok(counter)
}

pub fn try_create_int_gauge(name: &str, help: &str) -> Result<IntGauge, prometheus::Error> {
    let opts = Opts::new(name, help);
    let gauge = IntGauge::with_opts(opts)?;
    prometheus::register(Box::new(gauge.clone()))?;
    Ok(gauge)
}

pub fn try_create_gauge(name: &str, help: &str) -> Result<Gauge, prometheus::Error> {
    let opts = Opts::new(name, help);
    let gauge = Gauge::with_opts(opts)?;
    prometheus::register(Box::new(gauge.clone()))?;
    Ok(gauge)
}

lazy_static! {
    pub static ref BLOCK_PROCESSED_TOTAL: IntCounter =
        try_create_int_counter("total_blocks_processed", "Total number of blocks processed")
            .unwrap();
    pub static ref LAST_SEEN_BLOCK_HEIGHT: IntGauge = try_create_int_gauge(
        "last_seen_block_height",
        "latest block height seen by indexer."
    )
    .unwrap();
    pub static ref LATEST_BLOCK_TIMESTAMP_DIFF: Gauge = try_create_gauge(
        "latest_block_timestamp",
        "Difference between latest block timestamp and current time."
    )
    .unwrap();
}

#[get("/metrics")]
async fn get_metrics() -> impl Responder {
    let encoder = prometheus::TextEncoder::new();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        eprintln!("could not encode metrics: {}", e);
    };

    match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("custom metrics could not be from_utf8'd: {}", e);
            String::default()
        }
    }
}

pub(crate) async fn init_metrics_server() -> anyhow::Result<(), std::io::Error> {
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| String::from("3000"))
        .parse()
        .expect("Unable to parse `PORT`");

    tracing::info!(
        target: LOGGING_PREFIX,
        "Starting metrics server on http://0.0.0.0:{port}/metrics"
    );

    tracing::info!(
        target: LOGGING_PREFIX,
        "health probe on http://0.0.0.0:{port}/probe"
    );

    HttpServer::new(|| App::new().service(get_metrics))
        .bind(("0.0.0.0", port))?
        .run()
        .await
}
