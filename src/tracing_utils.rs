/// The resource representing a registered subscriber for non_blocking tracing logger
///
/// Once dropped, the subscriber is unregistered, and the output is flushed. Any messages output
/// after this value is dropped will be delivered to a previously active subscriber, if any.
#[allow(dead_code)]
pub struct DefaultSubcriberGuard {
    pub subscriber_guard: tracing::subscriber::DefaultGuard,
    pub writer_guard: tracing_appender::non_blocking::WorkerGuard,
}