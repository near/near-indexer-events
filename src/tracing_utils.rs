pub use {tracing, tracing_appender, tracing_subscriber};

use std::env;
use tracing_appender::non_blocking::NonBlocking;
use tracing_subscriber::filter::Filtered;
use tracing_subscriber::layer::{Layered, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{fmt, EnvFilter, Layer};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};

type LogLayer<Inner> = Layered<
    Filtered<
        fmt::Layer<Inner, fmt::format::DefaultFields, fmt::format::Format, NonBlocking>,
        EnvFilter,
        Inner,
    >,
    Inner,
>;

pub struct DefaultSubscriberGuard<S> {
    // We must first drop the `local_subscriber_guard` so that no new messages are delivered to
    // this subscriber while we take care of flushing the messages already in queue. If dropped the
    // other way around, the events/spans generated while the subscriber drop guard runs would be
    // lost.
    subscriber: Option<S>,
    #[allow(dead_code)]
    writer_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl<S: tracing::Subscriber + Send + Sync> DefaultSubscriberGuard<S> {
    /// Register this default subscriber globally , for all threads.
    ///
    /// Must not be called more than once. Mutually exclusive with `Self::local`.
    pub fn global(mut self) -> Self {
        if let Some(subscriber) = self.subscriber.take() {
            tracing::subscriber::set_global_default(subscriber)
                .expect("could not set a global subscriber");
        } else {
            panic!("trying to set a default subscriber that has been already taken")
        }
        self
    }
}

fn is_terminal() -> bool {
    // Crate `atty` provides a platform-independent way of checking whether the output is a tty.
    atty::is(atty::Stream::Stderr)
}

fn add_non_blocking_log_layer<S>(
    filter: EnvFilter,
    writer: NonBlocking,
    ansi: bool,
    subscriber: S,
) -> LogLayer<S>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
{
    let layer = fmt::layer()
        .with_ansi(ansi)
        .with_writer(writer)
        .with_filter(filter);

    subscriber.with(layer)
}   

pub fn default_subscriber_with_non_blocking_layer(
    env_filter: EnvFilter,
) -> DefaultSubscriberGuard<impl tracing::Subscriber + Send + Sync> {
    let color_output = std::env::var_os("NO_COLOR").is_none() && is_terminal();

    let stderr = std::io::stderr();
    let lined_stderr = std::io::LineWriter::new(stderr);
    let (writer, writer_guard) = tracing_appender::non_blocking(lined_stderr);
    
    let formatting_layer = BunyanFormattingLayer::new("indexer-events".into(), std::io::stdout);
    
       
    let subscriber = tracing_subscriber::registry();

    let subscriber = add_non_blocking_log_layer(env_filter, writer, color_output, subscriber);
    
    let subscriber = subscriber
                                                            .with(JsonStorageLayer)
                                                            .with(formatting_layer);
                                                            
    DefaultSubscriberGuard {
        subscriber: Some(subscriber),
        writer_guard: Some(writer_guard),
    }
}

pub async fn init_tracing(
    mut env_filter: EnvFilter,
) -> DefaultSubscriberGuard<impl tracing::Subscriber + Send + Sync> {
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

    let subscriber = default_subscriber_with_non_blocking_layer(env_filter);

    subscriber.global()
}
