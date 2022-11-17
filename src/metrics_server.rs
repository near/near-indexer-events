use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};
use opentelemetry::{
    global,
    metrics::{Counter, Histogram},
    sdk::{
        export::metrics::aggregation,
        metrics::{controllers, processors, selectors},
    },
    Context, KeyValue,
};
use opentelemetry_prometheus::PrometheusExporter;
use prometheus::{Encoder, TextEncoder};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::SystemTime;

// lazy_static! {
//     static ref HANDLER_ALL: [KeyValue; 1] = [KeyValue::new("handler", "all")];
// }

async fn serve_req(
    cx: Context,
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    println!("Receiving request at path {}", req.uri());
    let request_start = SystemTime::now();

    state.curr_block_no_counter.add(&cx, 1, &[]);

    let response = match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let mut buffer = vec![];
            let encoder = TextEncoder::new();
            let metric_families = state.exporter.registry().gather();
            encoder.encode(&metric_families, &mut buffer).unwrap();
            // state.http_body_gauge.record(&cx, buffer.len() as u64, &[]);

            Response::builder()
                .status(200)
                .header(CONTENT_TYPE, encoder.format_type())
                .body(Body::from(buffer))
                .unwrap()
        }
        (&Method::GET, "/") => Response::builder()
            .status(200)
            .body(Body::from("Hello World"))
            .unwrap(),
        _ => Response::builder()
            .status(404)
            .body(Body::from("Missing Page"))
            .unwrap(),
    };

        state.block_processing_duration_histogram.record(
        &cx,
        request_start.elapsed().map_or(0.0, |d| d.as_secs_f64()),
        &[],
    );
   
    Ok(response)
}

struct AppState {
    exporter: PrometheusExporter,
    curr_block_no_counter: Counter<u64>,
    block_processing_duration_histogram: Histogram<f64>,
}

#[tokio::main]
pub async fn init_metrics_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let controller = controllers::basic(
        processors::factory(
            selectors::simple::histogram([1.0, 2.0, 5.0, 10.0, 20.0, 50.0]),
            aggregation::cumulative_temporality_selector(),
        )
        .with_memory(true),
    )
    .build();

    let exporter = opentelemetry_prometheus::exporter(controller).init();
    let cx = Context::new();

    let meter = global::meter("ex.com/hyper");
    let state = Arc::new(AppState {
        exporter,
        curr_block_no_counter: meter
            .u64_counter("indexer.blocks_total")
            .with_description("Total number of blocks processed.")
            .init(),
        block_processing_duration_histogram: meter
            .f64_histogram("example.block_processing_duration_seconds")
            .with_description("The time it takes each block to process.")
            .init(),
    });

    // For every connection, we must make a `Service` to handle all
    // incoming HTTP requests on said connection.
    let make_svc = make_service_fn(move |_conn| {
        let state = state.clone();
        let cx = cx.clone();
       
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                serve_req(cx.clone(), req, state.clone())
            }))
        }
    });

    let addr = ([127, 0, 0, 1], 3000).into();

    let server = Server::bind(&addr).serve(make_svc);

    println!("Starting metrics server at http://{}", addr);

    server.await?;

    Ok(())
}
