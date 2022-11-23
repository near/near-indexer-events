use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};
use prometheus::{Encoder, Gauge, IntCounter, IntGauge, Opts};
use std::{convert::Infallible, env};

pub type Result<T, E> = std::result::Result<T, E>;
use chrono::{DateTime, NaiveDateTime, Utc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    pub static ref LATEST_BLOCK_HEIGHT: IntGauge = try_create_int_gauge(
        "latest_block_height",
        "Latest Block Height Of Indexer Thus Far"
    )
    .unwrap();
    pub static ref LATEST_BLOCK_TIMESTAMP: Gauge = try_create_gauge(
        "latest_block_timestamp",
        "Timestamp of when last block was indexed."
    )
    .unwrap();
}

enum ProbeResult {
    IndexerStarted,
    IndexerNotStarted,
    SomethingIsWrong,
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
            // Indexer should process at least 1 block within 2.5 minutes to report that it is working fine.
            let encoder = prometheus::TextEncoder::new();
            let mut probe_result = ProbeResult::IndexerNotStarted;
            let latest_block_timestamp = LATEST_BLOCK_TIMESTAMP.get();
            let last_known_blockheight = LATEST_BLOCK_HEIGHT.get() as u64;

            let time_since_the_epoch = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards");
            let time_elapsed = time_since_the_epoch
                .checked_sub(Duration::from_secs(latest_block_timestamp as u64))
                .unwrap();

            if latest_block_timestamp != 0.0 {
                match time_elapsed.as_secs().ge(&Duration::new(150, 0).as_secs()) {
                    true => probe_result = ProbeResult::SomethingIsWrong,
                    false => probe_result = ProbeResult::IndexerStarted,
                }
            }
            let mut res = "".to_owned();

            match probe_result {
                ProbeResult::IndexerNotStarted => res.push_str("Indexer has not started yet."),
                ProbeResult::IndexerStarted => {
                    res.push_str("Indexer is operating normally. Blocks reported at least every 2.5 minutes.");
                }
                ProbeResult::SomethingIsWrong => {
                    res.push_str("Something is wrong. Indexer has not reported any new blocks within 2.5 minutes.");
                }
            };

            if latest_block_timestamp != 0.0 {
                res.push_str("\n Last block height was: ");
                res.push_str(last_known_blockheight.to_string().as_str());
                res.push_str("\n last block was indexed at timestamp: ");
                let last_block_datetime =
                    NaiveDateTime::from_timestamp_opt(latest_block_timestamp as i64, 0);

                match last_block_datetime {
                    Some(datetime) => {
                        let dt = DateTime::<Utc>::from_utc(datetime, Utc);
                        res.push_str(&dt.format("%a %b %e %T %Y").to_string());
                    }
                    None => res.push_str("Error Converting time."),
                }
            }

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
    let make_svc = make_service_fn(move |_conn| async move {
        Ok::<_, Infallible>(service_fn(serve_req))
    });

    let port: u16 = match env::var("PORT") {
        Ok(val) => val.parse::<u16>().unwrap(),
        _ => 3000,
    };

    let addr = ([0, 0, 0, 0], port).into();

    let server = Server::bind(&addr).serve(make_svc);

    println!("Starting metrics server at http://{}/metrics", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
    Ok(())
}
