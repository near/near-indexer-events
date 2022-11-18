use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};
use prometheus::{Encoder, Histogram, HistogramOpts, IntCounter, Opts};
use std::{convert::Infallible, env};

pub type Result<T, E> = std::result::Result<T, E>;

pub fn try_create_histogram(name: &str, help: &str) -> Result<Histogram, prometheus::Error> {
    let opts = HistogramOpts::new(name, help);
    let histogram = Histogram::with_opts(opts)?;
    prometheus::register(Box::new(histogram.clone()))?;
    Ok(histogram)
}

/// Attempts to crate an `IntCounter`, returning `Err` if the registry does not accept the counter
/// (potentially due to naming conflict).
pub fn try_create_int_counter(name: &str, help: &str) -> Result<IntCounter, prometheus::Error> {
    let opts = Opts::new(name, help);
    let counter = IntCounter::with_opts(opts)?;
    prometheus::register(Box::new(counter.clone()))?;
    Ok(counter)
}

lazy_static! {
    pub static ref BLOCK_PROCESSED_TOTAL: IntCounter =
        try_create_int_counter("total_blocks_processed", "Total number of blocks processed")
            .unwrap();
    pub static ref BLOCK_PROCESSING_TIME: Histogram =
        try_create_histogram("block_processing_time", "Time spent indexing a block").unwrap();
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
        (&Method::GET, "/") => Response::builder()
            .status(200)
            .body(Body::from(""))
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
        Ok::<_, Infallible>(service_fn(move |req| serve_req(req)))
    });

    let port: u16 = match env::var("PORT") {
        Ok(val) => val.parse::<u16>().unwrap(),
        _ => 3000,
    };

    let addr = ([0, 0, 0, 0], port).into();

    let server = Server::bind(&addr).serve(make_svc);

    println!("Starting metrics server at http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
    Ok(())
}
