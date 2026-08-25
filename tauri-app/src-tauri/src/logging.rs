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

/// Visitor that retains the message and structured fields from a tracing event.
struct MessageVisitor {
    message: String,
    fields: Vec<String>,
}

impl MessageVisitor {
    fn new() -> Self {
        Self {
            message: String::new(),
            fields: Vec::new(),
        }
    }

    fn line(self) -> String {
        if self.fields.is_empty() {
            self.message
        } else if self.message.is_empty() {
            self.fields.join(" ")
        } else {
            format!("{} {}", self.message, self.fields.join(" "))
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push(format!("{}={value}", field.name()));
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

        let line = format!("{level} {target}: {}", visitor.line());

        self.ctx.push_log(line.clone());

        if *level <= Level::WARN {
            self.ctx.push_error(line);
        }
    }
}
