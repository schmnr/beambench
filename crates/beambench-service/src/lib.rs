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
    use std::sync::{LazyLock, Mutex, MutexGuard};

    use crate::persist;

    static PERSIST_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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
            // SAFETY: all service tests that mutate persistence env vars use
            // this crate-level guard, so env access is serialized.
            unsafe {
                std::env::set_var(persist::CONFIG_DIR_ENV, config_dir.path());
                std::env::set_var(persist::DATA_DIR_ENV, data_dir.path());
            }
            Self {
                _lock: lock,
                _config_dir: config_dir,
                _data_dir: data_dir,
            }
        }
    }

    impl Drop for PersistTestGuard {
        fn drop(&mut self) {
            // SAFETY: guarded by `PERSIST_TEST_LOCK` held for this guard's
            // lifetime.
            unsafe {
                std::env::remove_var(persist::CONFIG_DIR_ENV);
                std::env::remove_var(persist::DATA_DIR_ENV);
            }
        }
    }
}
