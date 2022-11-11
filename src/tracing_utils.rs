/// The resource representing a registered subscriber for non_blocking tracing logger
///
/// since the non_blocking_writer writes to a new thread "at a later point in time",
/// to ensure that all logs get flushed out in case of panics, we return the writer_guard
/// which when dropped, immediately flushes whatever non_blocking_writer has in its
/// cache to its intended place, that could be a file or a std_out.
///
/// https://docs.rs/tracing-appender/0.1.1/tracing_appender/non_blocking/struct.WorkerGuard.html
///
/// Once dropped, the subscriber is unregistered, and the output is flushed. Any messages output
/// after this value is dropped will be delivered to a previously active subscriber, if any.
#[allow(dead_code)]
pub struct DefaultSubcriberGuard {
    pub subscriber_guard: tracing::subscriber::DefaultGuard,
    pub writer_guard: tracing_appender::non_blocking::WorkerGuard,
}
