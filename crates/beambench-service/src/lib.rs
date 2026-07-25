//! Framework-agnostic service layer for Beam Bench.
//! Provides `ServiceContext` that owns all runtime state and can be shared
//! by Tauri (via `Arc`), an HTTP API, or a CLI.

pub mod agent;
pub mod context;
pub mod error;
pub mod events;
pub mod history;
mod lihuiyu_runtime;
pub mod material_apply;
pub mod ops;
pub mod persist;
mod ruida_runtime;
pub mod runtime;
pub mod validation;

pub use context::ServiceContext;
pub use error::{ServiceError, ServiceErrorCode, ServiceResult};
pub use events::{RuntimeSnapshot, ServiceEventEnvelope};
pub use history::{ProjectHistory, UndoState};
pub use material_apply::{MaterialApplyResponse, MaterialApplyWarning, MaterialApplyWarningCode};
pub use validation::{
    RoutingTarget, check_layer_content_invariant, effective_is_raster, resolve_layer_for_object,
};

#[cfg(test)]
pub(crate) mod test_support {
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex, MutexGuard};

    static PERSIST_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    thread_local! {
        static PERSISTENCE_ENABLED: Cell<bool> = const { Cell::new(false) };
        static PERSISTENCE_DIRS: RefCell<Option<(PathBuf, PathBuf)>> = const { RefCell::new(None) };
    }

    pub(crate) fn persistence_enabled_for_current_test() -> bool {
        PERSISTENCE_ENABLED.get()
    }

    pub(crate) fn persistence_config_dir_for_current_test() -> Option<PathBuf> {
        PERSISTENCE_DIRS.with(|dirs| dirs.borrow().as_ref().map(|(config, _)| config.clone()))
    }

    pub(crate) fn persistence_data_dir_for_current_test() -> Option<PathBuf> {
        PERSISTENCE_DIRS.with(|dirs| dirs.borrow().as_ref().map(|(_, data)| data.clone()))
    }

    pub(crate) struct PersistTestGuard {
        _lock: MutexGuard<'static, ()>,
        _config_dir: tempfile::TempDir,
        _data_dir: tempfile::TempDir,
    }

    impl PersistTestGuard {
        pub(crate) fn new() -> Self {
            let lock = PERSIST_TEST_LOCK.lock().unwrap();
            let config_dir = tempfile::tempdir().unwrap();
            let data_dir = tempfile::tempdir().unwrap();
            PERSISTENCE_DIRS.with(|dirs| {
                *dirs.borrow_mut() = Some((
                    config_dir.path().to_path_buf(),
                    data_dir.path().to_path_buf(),
                ));
            });
            PERSISTENCE_ENABLED.set(true);
            Self {
                _lock: lock,
                _config_dir: config_dir,
                _data_dir: data_dir,
            }
        }
    }

    impl Drop for PersistTestGuard {
        fn drop(&mut self) {
            PERSISTENCE_ENABLED.set(false);
            PERSISTENCE_DIRS.with(|dirs| *dirs.borrow_mut() = None);
        }
    }
}
