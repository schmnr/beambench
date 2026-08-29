use std::fmt;
use std::sync::Arc;

use beambench_service::ServiceContext;
use tracing::Level;
use tracing::field::{Field, Visit};

#[cfg(target_os = "linux")]
use std::fs::{File, OpenOptions};
#[cfg(target_os = "linux")]
use std::io::{self, Read, Write};
#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

#[cfg(target_os = "linux")]
pub fn install_startup_diagnostics() -> Option<PathBuf> {
    let path = beambench_service::persist::data_dir()?
        .join("diagnostics")
        .join("startup.log");
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!("Failed to create the startup diagnostics directory: {error}");
        return None;
    }

    let mut file = match OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) => {
            eprintln!("Failed to create the startup diagnostics log: {error}");
            return None;
        }
    };

    if let Err(error) = write_startup_header(&mut file) {
        eprintln!("Failed to write the startup diagnostics header: {error}");
    }
    let error_file = file.try_clone().ok();
    if let Err(error) = tee_stderr(file) {
        if let Some(mut error_file) = error_file {
            let _ = writeln!(error_file, "stderr capture failed: {error}");
        }
        eprintln!("Failed to capture startup diagnostics: {error}");
    }
    Some(path)
}

#[cfg(not(target_os = "linux"))]
pub fn install_startup_diagnostics() -> Option<std::path::PathBuf> {
    None
}

#[cfg(target_os = "linux")]
fn write_startup_header(file: &mut File) -> io::Result<()> {
    writeln!(file, "Beam Bench startup diagnostics")?;
    writeln!(file, "timestamp={}", chrono::Utc::now().to_rfc3339())?;
    writeln!(file, "app_version={}", beambench_buildinfo::APP_VERSION)?;
    writeln!(file, "build_target={}", beambench_buildinfo::TARGET_TRIPLE)?;
    writeln!(file, "git_sha={}", beambench_buildinfo::GIT_SHA)?;
    if let Ok(os_release) = std::fs::read_to_string("/etc/os-release") {
        for line in os_release
            .lines()
            .filter(|line| line.starts_with("PRETTY_NAME=") || line.starts_with("VERSION_ID="))
        {
            writeln!(file, "{line}")?;
        }
    }
    for key in [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
        "DESKTOP_SESSION",
        "GDK_BACKEND",
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ] {
        writeln!(
            file,
            "{key}={}",
            std::env::var_os(key)
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| "<unset>".to_owned())
        )?;
    }
    writeln!(file, "APPIMAGE={}", std::env::var_os("APPIMAGE").is_some())?;
    writeln!(file, "--- process output ---")?;
    file.flush()
}

/// Route stderr through a small tee so GTK, WebKit, EGL, and Rust diagnostics
/// are saved without taking terminal output away from advanced users.
#[cfg(target_os = "linux")]
fn tee_stderr(mut log: File) -> io::Result<()> {
    let mut pipe_fds = [0; 2];
    if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }

    let original_stderr = unsafe { libc::dup(libc::STDERR_FILENO) };
    if original_stderr >= 0 {
        unsafe {
            libc::fcntl(original_stderr, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
    unsafe {
        libc::fcntl(pipe_fds[0], libc::F_SETFD, libc::FD_CLOEXEC);
    }

    let mut reader = unsafe { File::from_raw_fd(pipe_fds[0]) };
    let mut terminal =
        (original_stderr >= 0).then(|| unsafe { File::from_raw_fd(original_stderr) });
    let thread = std::thread::Builder::new()
        .name("startup-diagnostics".to_owned())
        .spawn(move || {
            let mut buffer = [0_u8; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                let chunk = &buffer[..count];
                let _ = log.write_all(chunk);
                let _ = log.flush();
                if let Some(terminal) = terminal.as_mut() {
                    let _ = terminal.write_all(chunk);
                    let _ = terminal.flush();
                }
            }
        });
    if let Err(error) = thread {
        unsafe {
            libc::close(pipe_fds[1]);
        }
        return Err(error);
    }

    if unsafe { libc::dup2(pipe_fds[1], libc::STDERR_FILENO) } < 0 {
        let error = io::Error::last_os_error();
        unsafe {
            libc::close(pipe_fds[1]);
        }
        return Err(error);
    }
    unsafe {
        libc::close(pipe_fds[1]);
    }
    Ok(())
}

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
