//! Local HTTP API for Beam Bench.
//! Exposes the app's functionality via REST endpoints backed by `ServiceContext`.

pub mod config;
pub mod response;
pub mod routes;
pub mod server;

pub use config::ApiConfig;
pub use server::ApiServer;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{LazyLock, Mutex, MutexGuard};

    static PERSIST_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    pub(crate) struct PersistTestGuard {
        _lock: MutexGuard<'static, ()>,
        _dir: tempfile::TempDir,
    }

    impl PersistTestGuard {
        pub(crate) fn new() -> Self {
            let lock = PERSIST_TEST_LOCK.lock().unwrap();
            let dir = tempfile::tempdir().unwrap();
            // SAFETY: all API tests that mutate the shared persistence env use
            // this crate-level guard, so env access is serialized.
            unsafe {
                std::env::set_var(
                    beambench_service::persist::CONFIG_DIR_ENV,
                    dir.path().to_str().unwrap(),
                );
            }
            Self {
                _lock: lock,
                _dir: dir,
            }
        }
    }

    impl Drop for PersistTestGuard {
        fn drop(&mut self) {
            // SAFETY: guarded by `PERSIST_TEST_LOCK` held for this guard's
            // lifetime.
            unsafe {
                std::env::remove_var(beambench_service::persist::CONFIG_DIR_ENV);
            }
        }
    }
}
