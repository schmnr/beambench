//! An isolated dependency and API probe, never a desktop credential manager.
use keyring_core::{Entry, Result};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() {
    let native = match std::env::args().nth(1).as_deref() {
        None | Some("--mock") => false,
        Some("--native") => true,
        _ => {
            eprintln!("usage: beambench-credential-store-probe [--mock|--native]");
            std::process::exit(2);
        }
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("probe runtime");
        let _guard = runtime.enter();
        let _ = tx.send(round_trip(native).is_ok());
    });
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(true) => println!("credential_round_trip=ok"),
        Ok(false) if native => println!("credential_store=session_only"),
        Err(mpsc::RecvTimeoutError::Timeout) if native => {
            println!("credential_store=session_only_timeout");
        }
        _ => std::process::exit(1),
    }
}

fn round_trip(native: bool) -> Result<()> {
    if native {
        install_native_store()?;
    } else {
        keyring_core::set_default_store(keyring_core::mock::Store::new()?);
    }
    // Always use a disposable identity. Never print the test secret or raw OS errors.
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let account = format!("probe-{}-{nonce}", std::process::id());
    let entry = Entry::new("com.beambench.credential-probe", &account)?;
    let secret = format!("disposable-probe-{nonce}");
    let result = (|| {
        entry.set_password(&secret)?;
        let recovered = entry.get_password()?;
        if recovered != secret {
            return Err(keyring_core::Error::NoEntry);
        }
        Ok(())
    })();
    let cleanup = entry.delete_credential();
    keyring_core::unset_default_store();
    result.and(cleanup)
}

#[cfg(target_os = "linux")]
fn install_native_store() -> Result<()> {
    keyring_core::set_default_store(zbus_secret_service_keyring_store::Store::new()?);
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_native_store() -> Result<()> {
    keyring_core::set_default_store(apple_native_keyring_store::keychain::Store::new()?);
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_native_store() -> Result<()> {
    keyring_core::set_default_store(windows_native_keyring_store::Store::new()?);
    Ok(())
}
