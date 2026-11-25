//! Per-process storage for desktop copies. CLI and primary desktop paths stay compatible.
use std::path::PathBuf;
use std::sync::OnceLock;

static NUMBER: OnceLock<u32> = OnceLock::new();

pub fn initialize(number: u32) {
    NUMBER
        .set(number)
        .expect("desktop instance already initialized");
}

pub fn number() -> u32 {
    NUMBER.get().copied().unwrap_or(1)
}

pub fn storage_dir(base: PathBuf) -> PathBuf {
    match number() {
        number if number > 1 => base.join("instances").join(number.to_string()),
        _ => base,
    }
}
