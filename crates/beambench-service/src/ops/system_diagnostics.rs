//! Read-only host details for connection diagnostics. Never starts processes or services.

use beambench_common::feedback::DiagnosticSystem;

pub(super) fn collect(locale: Option<String>) -> DiagnosticSystem {
    #[cfg(target_os = "linux")]
    let mut system = {
        use std::sync::Mutex;
        use std::time::{Duration, Instant};
        static CACHE: Mutex<Option<(Instant, DiagnosticSystem)>> = Mutex::new(None);
        let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
        // ConnectionDiagnosticsPanel polls once a second for as long as it is
        // open. Whether a braille service is installed and running changes on
        // the order of a login, so re-probing every minute is ample.
        if cache
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() >= Duration::from_secs(60))
        {
            *cache = Some((Instant::now(), linux::read(std::path::Path::new("/"))));
        }
        cache.as_ref().unwrap().1.clone()
    };
    #[cfg(not(target_os = "linux"))]
    let mut system = DiagnosticSystem {
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        ..Default::default()
    };
    system.locale = locale;
    system
}

#[cfg(any(target_os = "linux", test))]
mod linux {
    use super::*;
    use std::fs::{self, File};
    use std::io::{BufRead, BufReader, Read};
    use std::path::Path;

    fn bounded_text(path: &Path, max: u64) -> Option<String> {
        let mut text = String::new();
        File::open(path)
            .ok()?
            .take(max + 1)
            .read_to_string(&mut text)
            .ok()?;
        (text.len() as u64 <= max).then_some(text)
    }

    fn brltty_version(root: &Path) -> Option<String> {
        let file = File::open(root.join("var/lib/dpkg/status")).ok()?;
        let mut package = false;
        let mut installed = false;
        let mut version = None;
        for line in BufReader::new(file.take(8 * 1024 * 1024)).lines() {
            let line = line.ok()?;
            if line.is_empty() {
                if package {
                    return installed.then_some(version).flatten();
                }
                installed = false;
                version = None;
            } else if let Some(value) = line.strip_prefix("Package: ") {
                package = value == "brltty";
            } else if package {
                if line == "Status: install ok installed" {
                    installed = true;
                }
                if let Some(value) = line.strip_prefix("Version: ")
                    && !value.is_empty()
                    && value.len() <= 128
                {
                    version = Some(value.to_owned());
                }
            }
        }
        (package && installed).then_some(version).flatten()
    }

    fn brltty_running(root: &Path) -> Option<bool> {
        let mut incomplete = false;
        for (index, entry) in fs::read_dir(root.join("proc")).ok()?.enumerate() {
            if index >= 16_384 {
                return None;
            }
            let entry = entry.ok()?;
            if !entry
                .file_name()
                .as_encoded_bytes()
                .iter()
                .all(u8::is_ascii_digit)
            {
                continue;
            }
            match fs::read_to_string(entry.path().join("comm")) {
                Ok(name) if name.trim() == "brltty" => return Some(true),
                Ok(_) => {}
                // Processes can exit during enumeration, and hardened /proc
                // mounts hide other users' entries. Neither means the probe failed.
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                    ) => {}
                Err(_) => incomplete = true,
            }
        }
        (!incomplete).then_some(false)
    }

    pub(super) fn read(root: &Path) -> DiagnosticSystem {
        let release = bounded_text(&root.join("etc/os-release"), 16 * 1024)
            .or_else(|| bounded_text(&root.join("usr/lib/os-release"), 16 * 1024));
        let running = brltty_running(root);
        DiagnosticSystem {
            os: "linux".to_owned(),
            os_version: release.and_then(|text| {
                text.lines().find_map(|line| {
                    line.strip_prefix("PRETTY_NAME=")
                        .map(|name| name.trim().trim_matches(['\"', '\'']).to_owned())
                        .filter(|name| !name.is_empty() && name.len() <= 256)
                })
            }),
            arch: std::env::consts::ARCH.to_owned(),
            brltty_running: running,
            // The version is only ever reported alongside a running process,
            // and /var/lib/dpkg/status is several megabytes and ~100k lines on
            // a typical install. Skip that parse unless it will be used.
            brltty_version: (running == Some(true))
                .then(|| brltty_version(root))
                .flatten(),
            ..Default::default()
        }
    }

    #[test]
    fn reads_linux_details_without_executing_configuration_or_exposing_processes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::create_dir_all(root.join("proc/123")).unwrap();
        fs::create_dir_all(root.join("var/lib/dpkg")).unwrap();
        fs::write(
            root.join("etc/os-release"),
            "PRETTY_NAME=\"Linux Mint 21.3\"\n",
        )
        .unwrap();
        fs::write(root.join("proc/123/comm"), "brltty\n").unwrap();
        fs::write(root.join("var/lib/dpkg/status"), "Package: unrelated\nVersion: 99\n\nPackage: brltty\nStatus: install ok installed\nVersion: 6.4-4ubuntu3\n\n").unwrap();
        let system = read(root);
        assert_eq!(system.os_version.as_deref(), Some("Linux Mint 21.3"));
        assert_eq!(system.brltty_running, Some(true));
        assert_eq!(system.brltty_version.as_deref(), Some("6.4-4ubuntu3"));
        fs::write(root.join("proc/123/comm"), "not-brltty\n").unwrap();
        let stopped = read(root);
        assert_eq!(stopped.brltty_running, Some(false));
        // The package database is not read at all when nothing is running.
        assert_eq!(stopped.brltty_version, None);

        // With the process back, a package that is only config-files still
        // reports no version.
        fs::write(root.join("proc/123/comm"), "brltty\n").unwrap();
        fs::write(
            root.join("var/lib/dpkg/status"),
            "Package: brltty\nStatus: deinstall ok config-files\nVersion: 6.4\n\n",
        )
        .unwrap();
        let running_without_package = read(root);
        assert_eq!(running_without_package.brltty_running, Some(true));
        assert_eq!(running_without_package.brltty_version, None);
    }

    #[test]
    fn unavailable_probe_is_unknown_and_oversized_metadata_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let system = read(dir.path());
        assert_eq!(system.brltty_running, None);
        assert_eq!(system.brltty_version, None);
        assert_eq!(system.os_version, None);
        fs::create_dir_all(dir.path().join("etc")).unwrap();
        fs::write(dir.path().join("etc/os-release"), "x".repeat(17 * 1024)).unwrap();
        assert_eq!(read(dir.path()).os_version, None);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn observes_brltty_named_process_through_real_procfs() {
        // This is a renamed sleep process, not the braille service. It exercises
        // the kernel's real /proc interface without opening any USB device.
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("brltty");
        fs::copy("/bin/sleep", &executable).unwrap();
        struct TestChild(std::process::Child);
        impl Drop for TestChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let child = TestChild(
            std::process::Command::new(executable)
                .arg("30")
                .spawn()
                .unwrap(),
        );
        let comm = format!("/proc/{}/comm", child.0.id());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while fs::read_to_string(&comm)
            .ok()
            .is_none_or(|name| name.trim() != "brltty")
        {
            assert!(
                std::time::Instant::now() < deadline,
                "child process did not appear in /proc"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let system = read(Path::new("/"));
        assert_eq!(system.os, "linux");
        assert_eq!(system.brltty_running, Some(true));
        assert!(system.os_version.is_some());
        drop(child);
        assert!(!Path::new(&comm).exists());
    }
}
