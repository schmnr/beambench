//! Validated, temporary GRBL output. Disk errors fail preparation before laser motion.
use crate::{GcodeConfig, GrblError, generate_gcode_to};
use beambench_planner::ExecutionPlan;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

pub struct GcodeSpool {
    reader: BufReader<File>,
    line_count: usize,
    bytes: u64,
}
fn spool_error(error: io::Error) -> GrblError {
    GrblError::GcodeError(format!("G-code spool I/O failed: {error}"))
}

/// Directory the spool file is created in.
///
/// The system temp directory is deliberately the last resort: `/tmp` is a
/// RAM-backed tmpfs on Debian, Ubuntu, Fedora, Arch and openSUSE, where
/// spooling would keep the whole job in memory (the exact cost the spool
/// exists to avoid) and can exhaust a small tmpfs mid-job. The user cache
/// directory is disk-backed on every supported platform.
fn spool_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("BEAMBENCH_SPOOL_DIR") {
        return Some(PathBuf::from(dir));
    }
    dirs::cache_dir()
        .or_else(dirs::data_dir)
        .map(|dir| dir.join("beam-bench").join("spool"))
}

/// Create the backing file, preferring disk-backed storage.
///
/// The file is unlinked immediately by `tempfile`, so it needs no cleanup and
/// cannot outlive the process even on a crash.
fn create_spool_file() -> Result<File, GrblError> {
    if let Some(dir) = spool_dir()
        && std::fs::create_dir_all(&dir).is_ok()
        && let Ok(file) = tempfile::tempfile_in(&dir)
    {
        return Ok(file);
    }
    // No usable cache directory (sandboxed, read-only home, full disk):
    // a temp-dir spool is still better than holding the job in a Vec.
    tempfile::tempfile().map_err(spool_error)
}
impl GcodeSpool {
    /// Spool for streaming to a GRBL controller: every line must fit the
    /// receive buffer, and the total is capped.
    pub fn generate(plan: &ExecutionPlan, config: &GcodeConfig) -> Result<Self, GrblError> {
        Self::generate_checked(plan, config, true)
    }

    /// Spool for writing a G-code file. The file may be run by other senders
    /// or controllers, so GRBL's streaming limits do not apply. Read it back
    /// with `copy_to`; `next_line` still enforces the streaming line limit.
    pub fn generate_for_file(plan: &ExecutionPlan, config: &GcodeConfig) -> Result<Self, GrblError> {
        Self::generate_checked(plan, config, false)
    }

    fn generate_checked(
        plan: &ExecutionPlan,
        config: &GcodeConfig,
        streaming: bool,
    ) -> Result<Self, GrblError> {
        let mut writer = BufWriter::new(create_spool_file()?);
        let mut line_count = 0;
        let mut bytes = 0;
        generate_gcode_to(plan, config, &mut |line| {
            if streaming && (line.len() >= 127 || line.contains(['\r', '\n'])) {
                return Err(GrblError::GcodeError(format!(
                    "G-code line {} cannot fit the GRBL receive buffer. Each line must be at most 126 bytes and contain no embedded newline.",
                    line_count + 1
                )));
            }
            if streaming && bytes + line.len() as u64 + 1 > 512 * 1024 * 1024 {
                return Err(GrblError::GcodeError("Generated output exceeds the 512 MiB spool budget. Reduce the job size or DPI.".into()));
            }
            if line_count > 0 {
                writer.write_all(b"\n").map_err(spool_error)?;
                bytes += 1;
            }
            writer.write_all(line.as_bytes()).map_err(spool_error)?;
            bytes += line.len() as u64;
            line_count += 1;
            Ok(())
        })?;
        writer.flush().map_err(spool_error)?;
        let mut file = writer
            .into_inner()
            .map_err(|e| spool_error(e.into_error()))?;
        file.rewind().map_err(spool_error)?;
        Ok(Self {
            reader: BufReader::new(file),
            line_count,
            bytes,
        })
    }
    pub fn len(&self) -> usize {
        self.line_count
    }
    pub fn is_empty(&self) -> bool {
        self.line_count == 0
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn next_line(&mut self) -> Result<Option<String>, GrblError> {
        let mut line = String::new();
        // Bound reads even if a spool is externally truncated or corrupted.
        let count = self
            .reader
            .by_ref()
            .take(128)
            .read_line(&mut line)
            .map_err(spool_error)?;
        if count == 0 {
            return Ok(None);
        }
        if line.ends_with('\n') {
            line.pop();
        }
        if line.len() >= 127 || line.contains(['\r', '\n']) {
            return Err(GrblError::GcodeError("Invalid line in G-code spool".into()));
        }
        Ok(Some(line))
    }
    /// Copy from the beginning, irrespective of the streaming cursor.
    ///
    /// The cursor is restored afterwards, so exporting a spool mid-stream does
    /// not truncate the job that is still being sent.
    pub fn copy_to(&mut self, writer: &mut impl Write) -> Result<u64, GrblError> {
        let resume_at = self.reader.stream_position().map_err(spool_error)?;
        self.reader.rewind().map_err(spool_error)?;
        let copied = io::copy(&mut self.reader, writer).map_err(spool_error);
        self.reader
            .seek(SeekFrom::Start(resume_at))
            .map_err(spool_error)?;
        copied
    }
}
