use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};
use prometheus::{Encoder, Gauge, IntCounter, IntGauge, Opts};
use std::{convert::Infallible, env};

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

async fn serve_req(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let response = match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let encoder = prometheus::TextEncoder::new();

            let mut buffer = Vec::new();
            if let Err(e) = encoder.encode(&prometheus::gather(), &mut buffer) {
                eprintln!("could not encode custom metrics: {}", e);
            };

            let res = match String::from_utf8(buffer.clone()) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("custom metrics could not be from_utf8'd: {}", e);
                    String::default()
                }
            };
            buffer.clear();

            Response::builder()
                .status(200)
                .header(CONTENT_TYPE, encoder.format_type())
                .body(Body::from(res))
                .unwrap()
        }
        (&Method::GET, "/probe") => {
            // shows the last seen block height and difference between last block_timestamp and now
            let encoder = prometheus::TextEncoder::new();
            let latest_block_timestamp_diff = LATEST_BLOCK_TIMESTAMP_DIFF.get();
            let last_seen_block_height = LAST_SEEN_BLOCK_HEIGHT.get();

            let mut res = "".to_owned();
            res.push_str("\n Last seen block height: ");
            res.push_str(last_seen_block_height.to_string().as_str());
            res.push_str("\n Last seen block timestamp and current time difference (in seconds): ");
            res.push_str(latest_block_timestamp_diff.to_string().as_str());

            Response::builder()
                .status(200)
                .header(CONTENT_TYPE, encoder.format_type())
                .body(Body::from(res))
                .unwrap()
        }
        (&Method::GET, "/") => Response::builder()
            .status(200)
            .body(Body::from("Service is running."))
            .unwrap(),
        _ => Response::builder()
            .status(404)
            .body(Body::from("Missing Page"))
            .unwrap(),
    };

    Ok(response)
}

#[tokio::main]
pub async fn init_metrics_server() -> std::result::Result<(), ()> {
    // For every connection, we must make a `Service` to handle all
    // incoming HTTP requests on said connection.
    let make_svc =
        make_service_fn(move |_conn| async move { Ok::<_, Infallible>(service_fn(serve_req)) });

    let port: u16 = match env::var("PORT") {
        Ok(val) => val.parse::<u16>().unwrap(),
        _ => 3000,
    };

    let addr = ([0, 0, 0, 0], port).into();

    let server = Server::bind(&addr).serve(make_svc);

    tracing::info!(target: LOGGING_PREFIX, "Starting metrics server at http://{}/metrics", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
    Ok(())
}
