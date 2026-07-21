use std::backtrace::Backtrace;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use beambench_common::feedback::{DiagnosticPanic, scrub_and_serialize};
use beambench_service::ServiceContext;
use chrono::Utc;

const PANIC_DIR_CAP: usize = 50;

pub fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Err(err) = write_panic_info(info) {
            eprintln!("Failed to write Beam Bench panic report: {err}");
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            previous_hook(info);
        }));
    }));
}

pub fn load_startup_panics_into_context(ctx: &Arc<ServiceContext>) {
    let Some(dir) = panics_dir() else {
        return;
    };
    let reports = beambench_service::ops::feedback::load_panic_reports_from_dir(&dir);
    ctx.set_panic_reports(reports);
    prune_panic_dir(&dir);
}

pub fn panics_dir() -> Option<PathBuf> {
    beambench_service::persist::data_dir().map(|path| path.join("panics"))
}

fn write_panic_info(info: &std::panic::PanicHookInfo<'_>) -> Result<(), String> {
    let message = panic_message(info);
    let location = info.location().map(|location| {
        format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        )
    });
    let report = DiagnosticPanic {
        ts: Utc::now().to_rfc3339(),
        thread: std::thread::current().name().map(str::to_owned),
        message,
        location,
        backtrace: Some(format!("{}", Backtrace::force_capture())),
        app_version: beambench_buildinfo::APP_VERSION.to_owned(),
        os: std::env::consts::OS.to_owned(),
        build_target: beambench_buildinfo::TARGET_TRIPLE.to_owned(),
        git_sha: beambench_buildinfo::GIT_SHA.to_owned(),
    };
    write_panic_report(&report)
}

fn write_panic_report(report: &DiagnosticPanic) -> Result<(), String> {
    let dir =
        panics_dir().ok_or_else(|| "Could not determine panic report directory".to_owned())?;
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("Failed to create panic directory: {err}"))?;
    let path = dir.join(panic_filename());
    let bytes = scrub_and_serialize(report)
        .map_err(|err| format!("Failed to serialize panic report: {err}"))?;
    std::fs::write(&path, bytes).map_err(|err| format!("Failed to write panic report: {err}"))?;
    prune_panic_dir(&dir);
    Ok(())
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(message) = info.payload().downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    if let Some(message) = info.payload().downcast_ref::<String>() {
        return message.clone();
    }
    "non-string panic payload".to_owned()
}

fn panic_filename() -> String {
    let now = Utc::now();
    let nanos = now.timestamp_subsec_nanos();
    format!(
        "{}-{}-{nanos}.json",
        now.format("%Y%m%dT%H%M%S%.3fZ"),
        std::process::id()
    )
}

fn prune_panic_dir(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            let modified = path
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()?;
            Some((path, modified))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    for (path, _) in entries.into_iter().skip(PANIC_DIR_CAP) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct DataDirGuard {
        _dir: TempDir,
        previous: Option<std::ffi::OsString>,
    }

    impl DataDirGuard {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let previous = std::env::var_os(beambench_service::persist::DATA_DIR_ENV);
            unsafe {
                std::env::set_var(beambench_service::persist::DATA_DIR_ENV, dir.path());
            }
            Self {
                _dir: dir,
                previous,
            }
        }
    }

    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(previous) = &self.previous {
                    std::env::set_var(beambench_service::persist::DATA_DIR_ENV, previous);
                } else {
                    std::env::remove_var(beambench_service::persist::DATA_DIR_ENV);
                }
            }
        }
    }

    #[test]
    fn panic_report_write_scrubs_paths() {
        let _guard = DataDirGuard::new();
        let report = DiagnosticPanic {
            ts: Utc::now().to_rfc3339(),
            thread: Some("test".to_owned()),
            message: "failed at /Users/alice/project/file.rs".to_owned(),
            location: Some("C:\\Users\\alice\\src\\main.rs:1:1".to_owned()),
            backtrace: None,
            app_version: "0.1.0".to_owned(),
            os: "macOS".to_owned(),
            build_target: "aarch64-apple-darwin".to_owned(),
            git_sha: "abc123".to_owned(),
        };

        write_panic_report(&report).unwrap();
        let dir = beambench_service::persist::data_dir()
            .unwrap()
            .join("panics");
        let content = std::fs::read_to_string(
            std::fs::read_dir(dir)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap();

        assert!(!content.contains("alice"));
        assert!(content.contains("<userhome>/project/file.rs"));
    }
}
