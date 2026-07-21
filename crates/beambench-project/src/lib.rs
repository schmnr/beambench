//! Project persistence for Beam Bench.
//! Manages .lzrproj file format (zip archive with JSON + assets).

pub mod persistence;
pub mod preferences;

pub use persistence::{
    PersistenceError, RecoveryInfo, check_recovery, discard_recovery, load_project, load_recovery,
    save_project, save_project_to_bytes, save_recovery,
};
pub use preferences::{PreferencesBundle, export_preferences, import_preferences};
