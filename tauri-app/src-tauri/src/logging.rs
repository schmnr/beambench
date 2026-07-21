use std::fmt;
use std::sync::Arc;

use beambench_service::ServiceContext;
use tracing::Level;
use tracing::field::{Field, Visit};

/// A `tracing_subscriber::Layer` that captures formatted log events into
/// [`ServiceContext::log_buffer`] and routes WARN/ERROR entries into
/// [`ServiceContext::active_errors`].
pub struct BufferLayer {
    ctx: Arc<ServiceContext>,
}

impl BufferLayer {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

/// Visitor that extracts the `message` field from a tracing event.
struct MessageVisitor {
    message: String,
}

impl MessageVisitor {
    fn new() -> Self {
        Self {
            message: String::new(),
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for BufferLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let level = metadata.level();
        let target = metadata.target();

        let mut visitor = MessageVisitor::new();
        event.record(&mut visitor);

        let line = format!("{level} {target}: {}", visitor.message);

        self.ctx.push_log(line.clone());

        if *level <= Level::WARN {
            self.ctx.push_error(line);
        }
    }
}
