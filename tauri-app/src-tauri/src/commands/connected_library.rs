//! Connected-library credentials and authenticated bytes stay in the native process.
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use beambench_common::connected_library::{
    LibraryAccountResponse, LibraryCollectionsResponse, LibraryFile, LibraryListingResponse,
    LibraryMimeType, LibraryProvider,
};
use beambench_core::ProjectObject;
use beambench_service::{ServiceContext, ops::imports};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tauri::{Emitter, Manager, State};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, oneshot},
};
use zeroize::{Zeroize, Zeroizing};

const API: &str = "/api/integrations/desktop/v1";
#[cfg(test)]
const CALLBACK: &str = "/oauth/craftgineer/callback";
fn origin(provider: LibraryProvider) -> &'static str {
    match provider {
        LibraryProvider::Craftgineer => "https://craftgineer.com",
        LibraryProvider::Printcutcarve => "https://printcutcarve.com",
    }
}
fn callback(provider: LibraryProvider) -> &'static str {
    match provider {
        LibraryProvider::Craftgineer => "/oauth/craftgineer/callback",
        LibraryProvider::Printcutcarve => "/oauth/printcutcarve/callback",
    }
}
const MAX_FILE: usize = 25 * 1024 * 1024;
pub struct ConnectedLibrary {
    craftgineer: ProviderConnection,
    printcutcarve: ProviderConnection,
}
impl Default for ConnectedLibrary {
    fn default() -> Self {
        Self {
            craftgineer: ProviderConnection::default(),
            printcutcarve: ProviderConnection {
                provider: LibraryProvider::Printcutcarve,
                ..Default::default()
            },
        }
    }
}
impl ConnectedLibrary {
    pub fn bind_events(&self, app: &tauri::AppHandle) {
        let _ = self.craftgineer.events.set(app.clone());
        let _ = self.printcutcarve.events.set(app.clone());
    }
    fn provider(&self, provider: LibraryProvider) -> &ProviderConnection {
        match provider {
            LibraryProvider::Craftgineer => &self.craftgineer,
            LibraryProvider::Printcutcarve => &self.printcutcarve,
        }
    }
}
type Result<T> = std::result::Result<T, String>;
fn error(code: &str) -> String {
    code.to_owned()
}

#[derive(Deserialize, Serialize)]
struct Saved {
    account_id: String,
    refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paid_through: Option<String>,
}
impl Drop for Saved {
    fn drop(&mut self) {
        self.refresh_token.zeroize();
    }
}
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    account_id: String,
    expires_in: u64,
    token_type: String,
    scope: String,
}
impl Drop for Tokens {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}
struct Connection {
    last_account: Option<LibraryAccountResponse>,
    tokens: Tokens,
    expires: tokio::time::Instant,
    remembered: bool,
    wallet_unavailable: bool,
    completed_inserts: std::collections::VecDeque<String>,
    inflight_inserts: std::collections::HashSet<String>,
}
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    account: Option<LibraryAccountResponse>,
    remembered: bool,
    wallet_unavailable: bool,
    native_generation: u64,
    paid_through: Option<String>,
}
#[derive(Clone, Serialize)]
struct ConnectionChange {
    provider: LibraryProvider,
    status: ConnectionStatus,
    error: Option<String>,
}

// One lazy worker per provider. A blocked wallet consumes at most that one thread;
// timeouts never start another. Generation checks remove late credential writes.
pub(crate) struct Vault {
    sender: std::sync::mpsc::SyncSender<VaultJob>,
    generation: Arc<AtomicU64>,
}
struct VaultJob {
    generation: u64,
    value: Option<Zeroizing<String>>,
    read: bool,
    reply: oneshot::Sender<Result<Option<Zeroizing<String>>>>,
}
pub(crate) fn install_credential_store() -> Result<()> {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new();
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new();
    keyring_core::set_default_store(store.map_err(|_| error("wallet_unavailable"))?);
    Ok(())
}
impl Vault {
    fn new(provider: LibraryProvider) -> Self {
        Self::with_provider(
            install_credential_store,
            "com.beambench.desktop-library",
            provider.id(),
        )
    }
    #[cfg(test)]
    fn with_initializer(initialize: fn() -> Result<()>, service: &'static str) -> Self {
        Self::with_provider(initialize, service, "craftgineer")
    }
    pub(crate) fn with_provider(
        initialize: fn() -> Result<()>,
        service: &'static str,
        user: &'static str,
    ) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<VaultJob>(1);
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = generation.clone();
        std::thread::Builder::new()
            .name("library-wallet".into())
            .spawn(move || {
                // Secret Service's async transport needs a runtime on this dedicated thread.
                #[cfg(target_os = "linux")]
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build();
                #[cfg(target_os = "linux")]
                let _entered = match runtime.as_ref() {
                    Ok(runtime) => Some(runtime.enter()),
                    Err(_) => None,
                };
                let mut initialized = false;
                for job in receiver {
                    let result = (|| {
                        if !initialized {
                            initialize()?;
                            initialized = true;
                        }
                        let entry = keyring_core::Entry::new(service, user)
                            .map_err(|_| error("wallet_unavailable"))?;
                        if worker_generation.load(Ordering::Acquire) != job.generation {
                            let _ = entry.delete_credential();
                            return Err(error("connection_changed"));
                        }
                        let value = if job.read {
                            match entry.get_password() {
                                Ok(value) => Some(Zeroizing::new(value)),
                                Err(keyring_core::Error::NoEntry) => None,
                                Err(_) => return Err(error("wallet_unavailable")),
                            }
                        } else {
                            if let Some(value) = job.value.as_ref() {
                                entry
                                    .set_password(value)
                                    .map_err(|_| error("wallet_unavailable"))?;
                            } else {
                                match entry.delete_credential() {
                                    Ok(()) | Err(keyring_core::Error::NoEntry) => (),
                                    Err(_) => return Err(error("wallet_unavailable")),
                                }
                            }
                            None
                        };
                        if worker_generation.load(Ordering::Acquire) != job.generation {
                            let _ = entry.delete_credential();
                            return Err(error("connection_changed"));
                        }
                        Ok(value)
                    })();
                    let _ = job.reply.send(result);
                }
            })
            .expect("create credential worker");
        Self { sender, generation }
    }
    pub(crate) async fn call(
        &self,
        value: Option<Zeroizing<String>>,
        read: bool,
    ) -> Result<Option<Zeroizing<String>>> {
        let generation = self.generation.load(Ordering::Acquire);
        let (reply, response) = oneshot::channel();
        self.sender
            .try_send(VaultJob {
                generation,
                value,
                read,
                reply,
            })
            .map_err(|_| error("wallet_unavailable"))?;
        match tokio::time::timeout(Duration::from_secs(8), response).await {
            Ok(Ok(result)) => result,
            _ => {
                self.generation.fetch_add(1, Ordering::AcqRel);
                Err(error("wallet_unavailable"))
            }
        }
    }
    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

#[derive(Default)]
struct ProviderConnection {
    provider: LibraryProvider,
    pending_connect: Arc<AtomicU64>,
    connection: Mutex<Option<Connection>>,
    generation: Arc<AtomicU64>,
    vault: OnceLock<Vault>,
    events: OnceLock<tauri::AppHandle>,
    paid_through: std::sync::Mutex<Option<String>>,
}
struct PendingConnect(Arc<AtomicU64>, u64);
impl Drop for PendingConnect {
    fn drop(&mut self) {
        let _ = self
            .0
            .compare_exchange(self.1, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}
impl ProviderConnection {
    fn paid_through(&self) -> Option<String> {
        self.paid_through
            .lock()
            .ok()
            .and_then(|value| valid_paid_through(value.clone()))
    }
    fn set_paid_through(&self, value: Option<String>) {
        if let Ok(mut current) = self.paid_through.lock() {
            *current = valid_paid_through(value);
        }
    }
    fn publish(&self, status: &ConnectionStatus) {
        self.publish_error(status, None);
    }
    fn publish_error(&self, status: &ConnectionStatus, error: Option<&str>) {
        if let Some(app) = self.events.get() {
            let mut status = status.clone();
            status.paid_through = self.paid_through();
            let _ = app.emit(
                "connected-library-changed",
                ConnectionChange {
                    provider: self.provider,
                    status,
                    error: error.map(str::to_owned),
                },
            );
        }
    }
    fn vault(&self) -> &Vault {
        self.vault.get_or_init(|| Vault::new(self.provider))
    }
    fn check(&self, generation: u64) -> Result<()> {
        if generation == self.generation.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(error("connection_changed"))
        }
    }
    async fn remember(&self, tokens: &Tokens) -> bool {
        let value = serde_json::to_string(&Saved {
            account_id: tokens.account_id.clone(),
            refresh_token: tokens.refresh_token.clone(),
            paid_through: self.paid_through(),
        });
        match value {
            Ok(value) => self
                .vault()
                .call(Some(Zeroizing::new(value)), false)
                .await
                .is_ok(),
            Err(_) => false,
        }
    }
}
fn valid_paid_through(value: Option<String>) -> Option<String> {
    value.filter(|value| {
        chrono::DateTime::parse_from_rfc3339(value).is_ok_and(|date| date > chrono::Utc::now())
    })
}
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(25))
        .https_only(true)
        .build()
        .map_err(|_| error("service_unavailable"))
}
async fn bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    match response.status().as_u16() {
        200 => (),
        400 => return Err(error("invalid_request")),
        401 => return Err(error("invalid_token")),
        403 => return Err(error("subscription_required")),
        404 => return Err(error("not_found")),
        412 => return Err(error("revision_conflict")),
        413 => return Err(error("file_too_large")),
        415 => return Err(error("unsupported_media_type")),
        429 => return Err(error("rate_limited")),
        _ => return Err(error("service_unavailable")),
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(error("file_too_large"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("service_unavailable"))?
    {
        if bytes.len() + chunk.len() > limit {
            return Err(error("file_too_large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
#[derive(Debug)]
enum TokenRequestError {
    // The request was not sent, or the server rejected it before rotating tokens.
    Retryable(String),
    // The credential is invalid, or rotation may have committed without a response.
    Discard(String),
}
impl TokenRequestError {
    fn code(self) -> String {
        match self {
            Self::Retryable(code) | Self::Discard(code) => code,
        }
    }
    fn refresh_code(self) -> String {
        match self {
            Self::Retryable(code) => code,
            Self::Discard(_) => error("invalid_token"),
        }
    }
}
async fn token_request(
    provider: LibraryProvider,
    fields: &[(&str, &str)],
) -> std::result::Result<Tokens, TokenRequestError> {
    receive_tokens(
        client()
            .map_err(TokenRequestError::Retryable)?
            .post(format!("{}{API}/token", origin(provider)))
            .form(fields),
    )
    .await
}
async fn receive_tokens(
    request: reqwest::RequestBuilder,
) -> std::result::Result<Tokens, TokenRequestError> {
    let response = request.send().await.map_err(|cause| {
        let code = error("service_unavailable");
        if cause.is_connect() {
            TokenRequestError::Retryable(code)
        } else {
            TokenRequestError::Discard(code)
        }
    })?;
    match response.status().as_u16() {
        400 => return Err(TokenRequestError::Discard(error("invalid_token"))),
        429 => return Err(TokenRequestError::Retryable(error("rate_limited"))),
        503 => return Err(TokenRequestError::Retryable(error("service_unavailable"))),
        _ => (),
    }
    let bytes = Zeroizing::new(
        bounded(response, 8192)
            .await
            .map_err(TokenRequestError::Discard)?,
    );
    let tokens: Tokens = serde_json::from_slice(&bytes)
        .map_err(|_| TokenRequestError::Discard(error("invalid_token")))?;
    let valid_secret = |s: &str| {
        s.len() == 43
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    if !valid_secret(&tokens.access_token)
        || !valid_secret(&tokens.refresh_token)
        || tokens.account_id.len() > 128
        || tokens.account_id.is_empty()
        || tokens.token_type != "Bearer"
        || tokens.scope != "library:read"
        || tokens.expires_in == 0
        || tokens.expires_in > 900
    {
        return Err(TokenRequestError::Discard(error("invalid_token")));
    }
    Ok(tokens)
}
async fn account(provider: LibraryProvider, tokens: &Tokens) -> Result<LibraryAccountResponse> {
    let response = client()?
        .get(format!("{}{API}/account", origin(provider)))
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|_| error("service_unavailable"))?;
    let account: LibraryAccountResponse =
        serde_json::from_slice(&bounded(response, 512 * 1024).await?)
            .map_err(|_| error("invalid_response"))?;
    account.validate_for_provider(provider).map_err(error)?;
    if account.account.id != tokens.account_id {
        return Err(error("library_account_mismatch"));
    }
    Ok(account)
}
async fn refreshed(manager: &ProviderConnection, connection: &mut Connection) -> Result<()> {
    if connection.expires > tokio::time::Instant::now() {
        return Ok(());
    }
    // Only a failure known to precede rotation leaves this token available for retry.
    if connection.tokens.refresh_token.is_empty() {
        return Err(error("invalid_token"));
    }
    let old_refresh = Zeroizing::new(std::mem::take(&mut connection.tokens.refresh_token));
    let result = token_request(
        manager.provider,
        &[
            ("client_id", "beambench-desktop"),
            ("grant_type", "refresh_token"),
            ("refresh_token", &old_refresh),
        ],
    )
    .await;
    complete_refresh(manager, connection, old_refresh, result).await
}
async fn complete_refresh(
    manager: &ProviderConnection,
    connection: &mut Connection,
    mut old_refresh: Zeroizing<String>,
    result: std::result::Result<Tokens, TokenRequestError>,
) -> Result<()> {
    let tokens = match result {
        Ok(tokens) => tokens,
        Err(TokenRequestError::Retryable(code)) => {
            connection.tokens.refresh_token = std::mem::take(&mut *old_refresh);
            return Err(code);
        }
        Err(TokenRequestError::Discard(_)) => {
            // An interrupted refresh may already have rotated on the server.
            // Drop the old secret rather than replaying it on the next request.
            connection.remembered = false;
            let _ = manager.vault().call(None, false).await;
            return Err(error("invalid_token"));
        }
    };
    if tokens.account_id != connection.tokens.account_id {
        return Err(error("library_account_mismatch"));
    }
    if connection.remembered {
        connection.remembered = manager.remember(&tokens).await;
        connection.wallet_unavailable = !connection.remembered;
    }
    connection.expires =
        tokio::time::Instant::now() + Duration::from_secs(tokens.expires_in.saturating_sub(30));
    connection.tokens = tokens;
    Ok(())
}
async fn restored(
    manager: &ProviderConnection,
    generation: u64,
    saved: Saved,
    result: std::result::Result<Tokens, TokenRequestError>,
) -> Result<Connection> {
    let tokens = result.map_err(TokenRequestError::refresh_code)?;
    if tokens.account_id != saved.account_id {
        return Err(error("library_account_mismatch"));
    }
    manager.check(generation)?;
    let remembered = manager.remember(&tokens).await;
    Ok(Connection {
        expires: tokio::time::Instant::now()
            + Duration::from_secs(tokens.expires_in.saturating_sub(30)),
        tokens,
        remembered,
        wallet_unavailable: !remembered,
        last_account: None,
        completed_inserts: Default::default(),
        inflight_inserts: Default::default(),
    })
}

#[tauri::command]
pub async fn connected_library_status(
    provider: LibraryProvider,
    manager: State<'_, ConnectedLibrary>,
    restore: bool,
) -> Result<ConnectionStatus> {
    let manager = manager.provider(provider);
    let mut generation = manager.generation.load(Ordering::Acquire);
    let mut guard = manager.connection.lock().await;
    manager.check(generation)?;
    if guard.is_none() && manager.pending_connect.load(Ordering::Acquire) != 0 {
        return Err(error("connection_changed"));
    }
    let result: Result<ConnectionStatus> = async {
        if guard.is_none() && restore {
            let saved = match manager.vault().call(None, true).await {
                Ok(value) => value,
                Err(_) => {
                    return Ok(ConnectionStatus {
                        wallet_unavailable: true,
                        ..Default::default()
                    });
                }
            };
            manager.check(generation)?;
            if let Some(saved) = saved {
                let saved: Saved =
                    serde_json::from_str(&saved).map_err(|_| error("invalid_token"))?;
                manager.set_paid_through(saved.paid_through.clone());
                manager.publish(&ConnectionStatus {
                    remembered: true,
                    native_generation: generation,
                    ..Default::default()
                });
                let result = token_request(
                    manager.provider,
                    &[
                        ("client_id", "beambench-desktop"),
                        ("grant_type", "refresh_token"),
                        ("refresh_token", &saved.refresh_token),
                    ],
                )
                .await;
                *guard = Some(restored(manager, generation, saved, result).await?);
            }
        }
        let Some(connection) = guard.as_mut() else {
            return Ok(ConnectionStatus::default());
        };
        refreshed(manager, connection).await?;
        let account = account(manager.provider, &connection.tokens).await?;
        manager.check(generation)?;
        let subscription_changed =
            manager.paid_through() != valid_paid_through(account.paid_through.clone());
        if subscription_changed
            || connection
                .last_account
                .as_ref()
                .is_some_and(|old| old.access != account.access)
        {
            manager
                .generation
                .compare_exchange(
                    generation,
                    generation + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .map_err(|_| error("connection_changed"))?;
            generation += 1;
        }
        manager.set_paid_through(account.paid_through.clone());
        if subscription_changed && connection.remembered {
            connection.remembered = manager.remember(&connection.tokens).await;
            connection.wallet_unavailable = !connection.remembered;
            manager.check(generation)?;
        }
        connection.last_account = Some(account.clone());
        Ok(ConnectionStatus {
            account: Some(account),
            remembered: connection.remembered,
            wallet_unavailable: connection.wallet_unavailable,
            native_generation: generation,
            paid_through: manager.paid_through(),
        })
    }
    .await;
    if restore
        && guard.is_none()
        && result.as_ref().is_err_and(|code| {
            matches!(code.as_str(), "invalid_token" | "library_account_mismatch")
        })
    {
        manager.set_paid_through(None);
        let failed_generation = manager.generation.fetch_add(1, Ordering::AcqRel) + 1;
        manager.publish_error(
            &ConnectionStatus {
                native_generation: failed_generation,
                ..Default::default()
            },
            result.as_ref().err().map(String::as_str),
        );
        manager.vault().invalidate();
        let _ = manager.vault().call(None, false).await;
    }
    let result = finish(manager, &mut guard, result).await;
    if let Ok(status) = &result {
        manager.check(generation)?;
        let mut status = status.clone();
        status.native_generation = generation;
        status.paid_through = manager.paid_through();
        manager.publish(&status);
        return Ok(status);
    }
    result
}

async fn finish<T>(
    manager: &ProviderConnection,
    guard: &mut Option<Connection>,
    result: Result<T>,
) -> Result<T> {
    if guard.is_some()
        && result.as_ref().is_err_and(|code| {
            matches!(code.as_str(), "invalid_token" | "library_account_mismatch")
        })
    {
        *guard = None;
        manager.set_paid_through(None);
        let generation = manager.generation.fetch_add(1, Ordering::AcqRel) + 1;
        manager.publish_error(
            &ConnectionStatus {
                native_generation: generation,
                ..Default::default()
            },
            result.as_ref().err().map(String::as_str),
        );
        manager.vault().invalidate();
        let _ = manager.vault().call(None, false).await;
    }
    result
}

struct Authorized {
    account_id: String,
    access_token: Zeroizing<String>,
}
async fn authorized(manager: &ProviderConnection, generation: u64) -> Result<Authorized> {
    let mut guard = manager.connection.lock().await;
    let result = async {
        manager.check(generation)?;
        let connection = guard.as_mut().ok_or_else(|| error("invalid_token"))?;
        // Refresh remains serialized, while ordinary downloads use an access-only snapshot.
        refreshed(manager, connection).await?;
        manager.check(generation)?;
        Ok(Authorized {
            account_id: connection.tokens.account_id.clone(),
            access_token: Zeroizing::new(connection.tokens.access_token.clone()),
        })
    }
    .await;
    finish(manager, &mut guard, result).await
}
async fn finish_request<T>(
    manager: &ProviderConnection,
    generation: u64,
    result: Result<T>,
) -> Result<T> {
    manager.check(generation)?;
    if result
        .as_ref()
        .is_err_and(|code| code == "subscription_required")
    {
        let mut guard = manager.connection.lock().await;
        manager.check(generation)?;
        if let Some(connection) = guard.as_mut()
            && let Some(account) = connection.last_account.as_mut()
        {
            let mut resulting_generation = generation;
            if account.access
                != beambench_common::connected_library::LibraryAccess::SubscriptionRequired
            {
                manager
                    .generation
                    .compare_exchange(
                        generation,
                        generation + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .map_err(|_| error("connection_changed"))?;
                resulting_generation += 1;
            }
            account.access =
                beambench_common::connected_library::LibraryAccess::SubscriptionRequired;
            manager.publish(&ConnectionStatus {
                account: Some(account.clone()),
                remembered: connection.remembered,
                wallet_unavailable: connection.wallet_unavailable,
                native_generation: resulting_generation,
                paid_through: manager.paid_through(),
            });
        }
    }
    if result
        .as_ref()
        .is_err_and(|code| matches!(code.as_str(), "invalid_token" | "library_account_mismatch"))
    {
        let mut guard = manager.connection.lock().await;
        manager.check(generation)?;
        return finish(manager, &mut guard, result).await;
    }
    result
}

fn random_secret() -> String {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}
fn callback_response(message: &str, accepted: bool) -> Vec<u8> {
    // The translated instruction contains no account data, code, state or credential.
    let escaped = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Beam Bench</title></head><body><h1>Beam Bench</h1><p>{}</p></body></html>",
        if accepted { &escaped } else { "Beam Bench" }
    );
    format!("HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        if accepted { "200 OK" } else { "400 Bad Request" }, body.len(), body).into_bytes()
}
fn callback_code(
    request: &str,
    port: u16,
    expected_state: &str,
    provider: LibraryProvider,
) -> Result<String> {
    let mut lines = request.split("\r\n");
    let line = lines.next().ok_or_else(|| error("invalid_callback"))?;
    let parts: Vec<_> = line.split(' ').collect();
    if parts.len() != 3
        || parts[0] != "GET"
        || parts[2] != "HTTP/1.1"
        || !parts[1].starts_with(&format!("{}?", callback(provider)))
    {
        return Err(error("invalid_callback"));
    }
    let host = format!("127.0.0.1:{port}");
    let hosts: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .collect();
    if hosts != [host.as_str()] {
        return Err(error("invalid_callback"));
    }
    let url = reqwest::Url::parse(&format!("http://{host}{}", parts[1]))
        .map_err(|_| error("invalid_callback"))?;
    let params: Vec<_> = url.query_pairs().collect();
    let values = |key: &str| {
        params
            .iter()
            .filter(|(name, _)| name == key)
            .map(|(_, value)| value.as_ref())
            .collect::<Vec<_>>()
    };
    if values("state") != [expected_state] {
        return Err(error("invalid_callback"));
    }
    if params
        .iter()
        .any(|(key, _)| !matches!(key.as_ref(), "state" | "code" | "error"))
    {
        return Err(error("invalid_callback"));
    }
    if values("error") == ["access_denied"] && values("code").is_empty() {
        return Err(error("access_denied"));
    }
    if values("error") == ["temporarily_unavailable"] && values("code").is_empty() {
        return Err(error("rate_limited"));
    }
    if !values("error").is_empty() {
        return Err(error("invalid_callback"));
    }
    let codes = values("code");
    if codes.len() != 1
        || codes[0].len() != 43
        || !codes[0]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(error("invalid_callback"));
    }
    Ok(codes[0].to_owned())
}
#[tauri::command]
pub async fn connected_library_connect(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    provider: LibraryProvider,
    manager: State<'_, ConnectedLibrary>,
    remember: bool,
    browser_message: String,
) -> Result<ConnectionStatus> {
    if browser_message.trim().is_empty() || browser_message.len() > 2048 {
        return Err(error("invalid_request"));
    }
    let manager = manager.provider(provider);
    let guard = manager.connection.lock().await;
    if guard.is_some() || manager.pending_connect.load(Ordering::Acquire) != 0 {
        return Err(error("already_connected"));
    }
    let generation = manager.generation.fetch_add(1, Ordering::AcqRel) + 1;
    manager.pending_connect.store(generation, Ordering::Release);
    let _pending = PendingConnect(manager.pending_connect.clone(), generation);
    drop(guard);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| error("loopback_unavailable"))?;
    let port = listener
        .local_addr()
        .map_err(|_| error("loopback_unavailable"))?
        .port();
    let redirect = format!("http://127.0.0.1:{port}{}", callback(manager.provider));
    let verifier = Zeroizing::new(random_secret());
    let state = random_secret();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = reqwest::Url::parse(&format!(
        "{}/integrations/desktop/authorize",
        origin(manager.provider)
    ))
    .map_err(|_| error("invalid_request"))?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", "beambench-desktop"),
        ("response_type", "code"),
        ("scope", "library:read"),
        ("redirect_uri", &redirect),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", &state),
    ]);
    crate::host_opener::open_url(&app, url.as_str()).map_err(|_| error("browser_unavailable"))?;
    let code = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            manager.check(generation)?;
            if window.is_visible().is_err() {
                return Err(error("connection_changed"));
            }
            let accepted =
                tokio::time::timeout(Duration::from_millis(250), listener.accept()).await;
            let Ok(Ok((mut stream, _))) = accepted else {
                continue;
            };
            let mut bytes = Vec::new();
            let read = tokio::time::timeout(Duration::from_secs(2), async {
                while bytes.len() < 8192 && !bytes.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    if stream.read(&mut byte).await.unwrap_or(0) == 0 {
                        break;
                    }
                    bytes.push(byte[0]);
                }
            })
            .await;
            if read.is_err() || !bytes.ends_with(b"\r\n\r\n") {
                continue;
            }
            let result = callback_code(
                &String::from_utf8_lossy(&bytes),
                port,
                &state,
                manager.provider,
            );
            let accepted = result.is_ok()
                || result
                    .as_ref()
                    .is_err_and(|code| matches!(code.as_str(), "access_denied" | "rate_limited"));
            let response = callback_response(&browser_message, accepted);
            let _ = tokio::time::timeout(Duration::from_secs(1), stream.write_all(&response)).await;
            match result {
                Ok(code) => return Ok(code),
                Err(code) if matches!(code.as_str(), "access_denied" | "rate_limited") => {
                    return Err(code);
                }
                Err(_) => continue,
            }
        }
    })
    .await
    .map_err(|_| error("connection_timeout"))??;
    manager.check(generation)?;
    let tokens = token_request(
        manager.provider,
        &[
            ("client_id", "beambench-desktop"),
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", &redirect),
        ],
    )
    .await
    .map_err(TokenRequestError::code)?;
    let account_result = account(manager.provider, &tokens).await;
    // Serialize the final wallet write and commit with disconnect. Cancellation
    // can invalidate immediately; its cleanup cannot finish ahead of this write.
    let mut guard = manager.connection.lock().await;
    let mut wallet_touched = false;
    let result = async {
        let account = account_result?;
        manager.check(generation)?;
        if window.is_visible().is_err() {
            return Err(error("connection_changed"));
        }
        wallet_touched = remember;
        manager.set_paid_through(account.paid_through.clone());
        let remembered = remember && manager.remember(&tokens).await;
        manager.check(generation)?;
        if window.is_visible().is_err() {
            return Err(error("connection_changed"));
        }
        window
            .unminimize()
            .map_err(|_| error("connection_changed"))?;
        window
            .set_focus()
            .map_err(|_| error("connection_changed"))?;
        manager.check(generation)?;
        Ok::<_, String>((account, remembered))
    }
    .await;
    let (account, remembered) = match result {
        Ok(value) => value,
        Err(code) => {
            manager.set_paid_through(None);
            if wallet_touched {
                manager.vault().invalidate();
                let _ = manager.vault().call(None, false).await;
            }
            let _ = revoke_remote(manager.provider, &tokens.refresh_token).await;
            return Err(code);
        }
    };
    *guard = Some(Connection {
        expires: tokio::time::Instant::now()
            + Duration::from_secs(tokens.expires_in.saturating_sub(30)),
        tokens,
        remembered,
        wallet_unavailable: remember && !remembered,
        last_account: Some(account.clone()),
        completed_inserts: Default::default(),
        inflight_inserts: Default::default(),
    });
    let status = ConnectionStatus {
        account: Some(account),
        remembered,
        wallet_unavailable: remember && !remembered,
        native_generation: generation,
        paid_through: manager.paid_through(),
    };
    manager.publish(&status);
    Ok(status)
}
#[tauri::command]
pub async fn connected_library_cancel(
    provider: LibraryProvider,
    manager: State<'_, ConnectedLibrary>,
) -> Result<()> {
    connected_library_disconnect(provider, manager).await
}
async fn revoke_remote(provider: LibraryProvider, token: &str) -> bool {
    tokio::time::timeout(Duration::from_secs(4), async {
        client()?
            .post(format!("{}{API}/revoke", origin(provider)))
            .form(&[("client_id", "beambench-desktop"), ("token", token)])
            .send()
            .await
            .map_err(|_| error("service_unavailable"))?
            .error_for_status()
            .map_err(|_| error("service_unavailable"))?;
        Ok::<_, String>(())
    })
    .await
    .is_ok_and(|result| result.is_ok())
}
#[tauri::command]
pub async fn connected_library_disconnect(
    provider: LibraryProvider,
    manager: State<'_, ConnectedLibrary>,
) -> Result<()> {
    let manager = manager.provider(provider);
    manager.pending_connect.store(0, Ordering::Release);
    manager.set_paid_through(None);
    let generation = manager.generation.fetch_add(1, Ordering::AcqRel) + 1;
    if let Some(vault) = manager.vault.get() {
        vault.invalidate();
    }
    let mut guard = manager.connection.lock().await;
    let connection = guard.take();
    // A restore may have been awaiting its wallet read when disconnect began.
    // Clear again under the connection lock before publishing the final state.
    manager.set_paid_through(None);
    manager.publish(&ConnectionStatus {
        native_generation: generation,
        ..Default::default()
    });
    let removed = manager.vault().call(None, false).await.is_ok();
    let revoked = if let Some(connection) = connection {
        revoke_remote(manager.provider, &connection.tokens.refresh_token).await
    } else {
        true
    };
    if !removed {
        Err(error("wallet_cleanup_required"))
    } else if !revoked {
        Err(error("revocation_pending"))
    } else {
        Ok(())
    }
}
#[tauri::command]
pub async fn connected_library_collections(
    provider: LibraryProvider,
    manager: State<'_, ConnectedLibrary>,
    q: String,
    cursor: Option<String>,
) -> Result<LibraryCollectionsResponse> {
    if provider != LibraryProvider::Printcutcarve
        || q.chars().count() > 120
        || cursor.as_ref().is_some_and(|value| value.len() > 2048)
    {
        return Err(error("invalid_request"));
    }
    let manager = manager.provider(provider);
    let generation = manager.generation.load(Ordering::Acquire);
    let auth = authorized(manager, generation).await?;
    let result = async {
        let mut url = reqwest::Url::parse(&format!("{}{API}/collections", origin(provider)))
            .map_err(|_| error("invalid_request"))?;
        url.query_pairs_mut()
            .append_pair("q", &q)
            .append_pair("limit", "24");
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("cursor", &cursor);
        }
        let response = client()?
            .get(url)
            .bearer_auth(auth.access_token.as_str())
            .send()
            .await
            .map_err(|_| error("service_unavailable"))?;
        let listing: LibraryCollectionsResponse =
            serde_json::from_slice(&bounded(response, 512 * 1024).await?)
                .map_err(|_| error("invalid_response"))?;
        listing
            .validate_for(provider, &auth.account_id)
            .map_err(error)?;
        Ok(listing)
    }
    .await;
    finish_request(manager, generation, result).await
}
#[tauri::command]
pub async fn connected_library_files(
    provider: LibraryProvider,
    manager: State<'_, ConnectedLibrary>,
    q: String,
    cursor: Option<String>,
    collection: Option<String>,
) -> Result<LibraryListingResponse> {
    if q.chars().count() > 120
        || cursor.as_ref().is_some_and(|value| value.len() > 2048)
        || (provider == LibraryProvider::Printcutcarve
            && !collection.as_ref().is_some_and(|value| {
                !value.is_empty()
                    && value.len() <= 128
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            }))
        || (provider == LibraryProvider::Craftgineer && collection.is_some())
    {
        return Err(error("invalid_request"));
    }
    let manager = manager.provider(provider);
    let generation = manager.generation.load(Ordering::Acquire);
    let auth = authorized(manager, generation).await?;
    let result = async {
        let mut url = reqwest::Url::parse(&format!("{}{API}/files", origin(provider)))
            .map_err(|_| error("invalid_request"))?;
        url.query_pairs_mut()
            .append_pair("q", &q)
            .append_pair("limit", "24");
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("cursor", &cursor);
        }
        if let Some(collection) = &collection {
            url.query_pairs_mut().append_pair("collection", collection);
        }
        let response = client()?
            .get(url)
            .bearer_auth(auth.access_token.as_str())
            .send()
            .await
            .map_err(|_| error("service_unavailable"))?;
        let listing: LibraryListingResponse =
            serde_json::from_slice(&bounded(response, 512 * 1024).await?)
                .map_err(|_| error("invalid_response"))?;
        listing
            .validate_for_provider(provider, &auth.account_id)
            .map_err(error)?;
        if let Some(collection) = &collection {
            if listing
                .files
                .iter()
                .any(|file| file.collection.as_ref().map(|value| &value.id) != Some(collection))
            {
                return Err(error("invalid_library_collection"));
            }
        }
        Ok(listing)
    }
    .await;
    finish_request(manager, generation, result).await
}
#[tauri::command]
pub fn connected_library_open_website(
    app: tauri::AppHandle,
    provider: LibraryProvider,
    slug: Option<String>,
) -> Result<()> {
    let suffix = match slug {
        Some(slug)
            if provider == LibraryProvider::Printcutcarve
                && !slug.is_empty()
                && slug.len() <= 200
                && slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') =>
        {
            format!("/account/downloads/{slug}")
        }
        Some(_) => return Err(error("invalid_request")),
        None if provider == LibraryProvider::Printcutcarve => "/account/downloads".to_owned(),
        None => "/files".to_owned(),
    };
    crate::host_opener::open_url(&app, &format!("{}{suffix}", origin(provider)))
        .map_err(|_| error("browser_unavailable"))
}
async fn file_bytes(
    provider: LibraryProvider,
    auth: &Authorized,
    file: &LibraryFile,
    thumbnail: bool,
) -> Result<Vec<u8>> {
    // Validate even when the renderer supplied this metadata.
    LibraryListingResponse {
        schema_version: 1,
        provider,
        account_id: auth.account_id.clone(),
        files: vec![file.clone()],
        next_cursor: None,
        unavailable_files: Vec::new(),
    }
    .validate_for_provider(provider, &auth.account_id)
    .map_err(error)?;
    let suffix = if thumbnail { "thumbnail" } else { "content" };
    let response = client()?
        .get(format!(
            "{}{API}/files/{}/{suffix}",
            origin(provider),
            file.id
        ))
        .bearer_auth(auth.access_token.as_str())
        .header("If-Match", format!("\"{}\"", file.revision))
        .send()
        .await
        .map_err(|_| error("service_unavailable"))?;
    let expected = if thumbnail {
        "image/webp"
    } else {
        mime(&file.mime_type).0
    };
    if response.status().is_success()
        && response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            != Some(expected)
    {
        return Err(error("unsupported_media_type"));
    }
    let bytes = bounded(response, if thumbnail { 1024 * 1024 } else { MAX_FILE }).await?;
    if !thumbnail
        && (bytes.len() as u64 != file.size_bytes
            || format!("sha256:{:x}", Sha256::digest(&bytes)) != file.revision)
    {
        return Err(error("revision_conflict"));
    }
    Ok(bytes)
}
fn mime(value: &LibraryMimeType) -> (&'static str, &'static str) {
    match value {
        LibraryMimeType::Svg => ("image/svg+xml", "svg"),
        LibraryMimeType::Png => ("image/png", "png"),
        LibraryMimeType::Jpeg => ("image/jpeg", "jpg"),
        LibraryMimeType::Webp => ("image/webp", "webp"),
    }
}
#[tauri::command]
pub async fn connected_library_thumbnail(
    provider: LibraryProvider,
    manager: State<'_, ConnectedLibrary>,
    file: LibraryFile,
) -> Result<String> {
    let manager = manager.provider(provider);
    let generation = manager.generation.load(Ordering::Acquire);
    let auth = authorized(manager, generation).await?;
    let result = async {
        let bytes = file_bytes(manager.provider, &auth, &file, true).await?;
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::WebP);
        let (width, height) = reader
            .into_dimensions()
            .map_err(|_| error("invalid_response"))?;
        if width == 0 || height == 0 || width > 2048 || height > 2048 {
            return Err(error("invalid_response"));
        }
        manager.check(generation)?;
        Ok(format!("data:image/webp;base64,{}", STANDARD.encode(bytes)))
    }
    .await;
    finish_request(manager, generation, result).await
}
#[tauri::command]
pub fn connected_library_prepare_insert(
    svc: State<'_, Arc<ServiceContext>>,
    project_id: String,
) -> Result<u64> {
    imports::connected_library_document_epoch(&svc, &project_id).map_err(Into::into)
}
#[tauri::command]
pub async fn connected_library_insert(
    window: tauri::WebviewWindow,
    provider: LibraryProvider,
    manager: State<'_, ConnectedLibrary>,
    svc: State<'_, Arc<ServiceContext>>,
    file: LibraryFile,
    project_id: String,
    expected_epoch: u64,
    request_id: String,
    layer_id: String,
    create_layer: Option<beambench_service::ops::project::AddObjectLayerInput>,
) -> Result<Vec<ProjectObject>> {
    let manager = manager.provider(provider);
    if request_id.len() != 36 || uuid::Uuid::parse_str(&request_id).is_err() {
        return Err(error("invalid_request"));
    }
    let epoch = expected_epoch;
    let generation = manager.generation.load(Ordering::Acquire);
    let auth = authorized(manager, generation).await?;
    {
        let mut guard = manager.connection.lock().await;
        manager.check(generation)?;
        let connection = guard.as_mut().ok_or_else(|| error("invalid_token"))?;
        if connection.completed_inserts.contains(&request_id) {
            return Ok(Vec::new());
        }
        if connection.inflight_inserts.contains(&request_id) {
            return Err(error("request_in_progress"));
        }
        if connection.inflight_inserts.len() >= 32 {
            return Err(error("rate_limited"));
        }
        connection.inflight_inserts.insert(request_id.clone());
    }
    let result = async {
        let bytes = file_bytes(manager.provider, &auth, &file, false).await?;
        let mut guard = manager.connection.lock().await;
        manager.check(generation)?;
        let connection = guard.as_mut().ok_or_else(|| error("invalid_token"))?;
        if window.is_visible().is_err() {
            return Err(error("connection_changed"));
        }
        let stem = std::path::Path::new(&file.name)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("artwork");
        let stem: String = stem
            .chars()
            .filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':'))
            .take(120)
            .collect();
        let filename = format!(
            "{}.{}",
            if stem.is_empty() { "artwork" } else { &stem },
            mime(&file.mime_type).1
        );
        let layer_id = super::project::parse_id(&layer_id).map_err(|_| error("invalid_request"))?;
        let svc = svc.inner().clone();
        let app = window.app_handle().clone();
        let window_label = window.label().to_owned();
        let connection_generation = manager.generation.clone();
        let objects = tokio::task::spawn_blocking(move || {
            let still_current = || {
                connection_generation.load(Ordering::Acquire) == generation
                    && app.get_webview_window(&window_label).is_some()
            };
            imports::import_connected_library_file(
                &svc,
                &filename,
                bytes,
                layer_id,
                create_layer,
                imports::ConnectedLibraryDestination {
                    project_id: &project_id,
                    epoch,
                    still_current: &still_current,
                },
            )
            .map_err(String::from)
        })
        .await
        .map_err(|_| error("import_failed"))??;
        connection.completed_inserts.push_back(request_id.clone());
        if connection.completed_inserts.len() > 128 {
            connection.completed_inserts.pop_front();
        }
        Ok(objects)
    }
    .await;
    {
        let mut guard = manager.connection.lock().await;
        if manager.check(generation).is_ok() {
            if let Some(connection) = guard.as_mut() {
                connection.inflight_inserts.remove(&request_id);
            }
        }
    }
    finish_request(manager, generation, result).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mock_store() -> Result<()> {
        static INITIALIZED: OnceLock<()> = OnceLock::new();
        INITIALIZED.get_or_init(|| {
            keyring_core::set_default_store(
                keyring_core::mock::Store::new().expect("create mock credential store"),
            );
        });
        Ok(())
    }
    fn fixture_tokens(refresh: char) -> Tokens {
        Tokens {
            access_token: "a".repeat(43),
            refresh_token: refresh.to_string().repeat(43),
            account_id: "disposable-test-account".into(),
            expires_in: 900,
            token_type: "Bearer".into(),
            scope: "library:read".into(),
        }
    }
    #[tokio::test]
    async fn remembered_paid_period_is_bounded_and_revocation_removes_it() {
        let manager = ProviderConnection {
            vault: OnceLock::from(Vault::with_initializer(
                mock_store,
                "com.beambench.desktop-library.test.paid-cache",
            )),
            ..Default::default()
        };
        let legacy: Saved = serde_json::from_value(serde_json::json!({
            "account_id": "disposable-test-account", "refresh_token": "r".repeat(43)
        }))
        .unwrap();
        assert!(legacy.paid_through.is_none());
        let until = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        manager.set_paid_through(Some(until.clone()));
        assert!(manager.remember(&fixture_tokens('p')).await);
        let stored = manager.vault().call(None, true).await.unwrap().unwrap();
        let saved: Saved = serde_json::from_str(&stored).unwrap();
        assert_eq!(saved.paid_through.as_deref(), Some(until.as_str()));
        // An offline restore preserves the last verified period without granting file access.
        let result = restored(
            &manager,
            0,
            saved,
            Err(TokenRequestError::Retryable(error("service_unavailable"))),
        )
        .await;
        assert!(matches!(result, Err(code) if code == "service_unavailable"));
        assert_eq!(manager.paid_through().as_deref(), Some(until.as_str()));
        manager.set_paid_through(Some(
            (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339(),
        ));
        assert!(manager.paid_through().is_none());
        manager.set_paid_through(Some(until));
        let mut connection = Some(Connection {
            tokens: fixture_tokens('p'),
            expires: tokio::time::Instant::now(),
            remembered: true,
            wallet_unavailable: false,
            last_account: None,
            completed_inserts: Default::default(),
            inflight_inserts: Default::default(),
        });
        assert_eq!(
            finish::<()>(&manager, &mut connection, Err(error("invalid_token"))).await,
            Err(error("invalid_token"))
        );
        assert!(connection.is_none());
        assert!(manager.paid_through().is_none());
        assert!(manager.vault().call(None, true).await.unwrap().is_none());
    }
    #[tokio::test]
    async fn access_loss_invalidates_pending_work_only_for_its_provider() {
        let manager = ConnectedLibrary::default();
        let pcc = manager.provider(LibraryProvider::Printcutcarve);
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/beambench-common/tests/fixtures/printcutcarve_library_v1.json"
        ))
        .unwrap();
        let account: LibraryAccountResponse =
            serde_json::from_value(fixture["accountResponse"].clone()).unwrap();
        *pcc.connection.lock().await = Some(Connection {
            tokens: fixture_tokens('p'),
            expires: tokio::time::Instant::now() + Duration::from_secs(900),
            remembered: false,
            wallet_unavailable: false,
            last_account: Some(account),
            completed_inserts: Default::default(),
            inflight_inserts: Default::default(),
        });
        assert_eq!(
            finish_request::<()>(pcc, 0, Err(error("subscription_required"))).await,
            Err(error("subscription_required"))
        );
        assert_eq!(pcc.check(0), Err(error("connection_changed")));
        assert_eq!(manager.craftgineer.check(0), Ok(()));
        let guard = pcc.connection.lock().await;
        let connection = guard.as_ref().unwrap();
        assert_eq!(
            connection.last_account.as_ref().unwrap().access,
            beambench_common::connected_library::LibraryAccess::SubscriptionRequired
        );
        assert_eq!(connection.tokens.refresh_token, "p".repeat(43));
    }
    #[tokio::test]
    async fn providers_keep_separate_credentials_and_connection_locks() {
        let manager = ConnectedLibrary {
            craftgineer: ProviderConnection {
                vault: OnceLock::from(Vault::with_provider(
                    mock_store,
                    "com.beambench.desktop-library.test.providers",
                    "craftgineer",
                )),
                ..Default::default()
            },
            printcutcarve: ProviderConnection {
                provider: LibraryProvider::Printcutcarve,
                vault: OnceLock::from(Vault::with_provider(
                    mock_store,
                    "com.beambench.desktop-library.test.providers",
                    "printcutcarve",
                )),
                ..Default::default()
            },
        };
        let cg = manager.provider(LibraryProvider::Craftgineer);
        let pcc = manager.provider(LibraryProvider::Printcutcarve);
        assert!(cg.remember(&fixture_tokens('c')).await);
        assert!(pcc.remember(&fixture_tokens('p')).await);
        let stored = cg.vault().call(None, true).await.unwrap().unwrap();
        let stored: Saved = serde_json::from_str(&stored).unwrap();
        assert_eq!(stored.refresh_token, "c".repeat(43));
        pcc.vault().call(None, false).await.unwrap();
        assert!(pcc.vault().call(None, true).await.unwrap().is_none());
        assert!(cg.vault().call(None, true).await.unwrap().is_some());
        let _slow_cg = cg.connection.lock().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), pcc.connection.lock())
                .await
                .is_ok()
        );
        pcc.generation.fetch_add(1, Ordering::AcqRel);
        assert!(cg.check(0).is_ok());
        cg.vault().call(None, false).await.unwrap();
    }

    #[test]
    fn callback_response_escapes_translated_text_and_replaces_the_consent_page() {
        let response = String::from_utf8(callback_response(
            "Return <script>bad</script> & check",
            true,
        ))
        .unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("&lt;script&gt;bad&lt;/script&gt; &amp;"));
        assert!(!response.contains("<script>"));
        assert!(response.contains("Cache-Control: no-store"));
        assert!(response.contains("frame-ancestors 'none'"));
        let invalid = String::from_utf8(callback_response("Return", false)).unwrap();
        assert!(invalid.starts_with("HTTP/1.1 400 Bad Request"));
    }
    async fn token_response(
        status: u16,
        body: String,
    ) -> std::result::Result<Tokens, TokenRequestError> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut bytes = [0; 1024];
                let count = stream.read(&mut bytes).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&bytes[..count]);
                if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    break;
                }
            }
            // status 0 simulates losing the response after receiving the request.
            if status != 0 {
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len(),
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let result = receive_tokens(
            reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap()
                .post(format!("http://{address}/token")),
        )
        .await;
        server.await.unwrap();
        result
    }
    async fn connection_failure() -> std::result::Result<Tokens, TokenRequestError> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        receive_tokens(
            reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap()
                .post(format!("http://{address}/token")),
        )
        .await
    }

    #[tokio::test]
    async fn remembered_refresh_survives_offline_and_pre_rotation_rejections() {
        let manager = ProviderConnection {
            vault: OnceLock::from(Vault::with_initializer(
                mock_store,
                "com.beambench.desktop-library.test.retry",
            )),
            ..Default::default()
        };
        let mut connection = Connection {
            tokens: fixture_tokens('r'),
            expires: tokio::time::Instant::now(),
            remembered: true,
            wallet_unavailable: false,
            last_account: None,
            completed_inserts: Default::default(),
            inflight_inserts: Default::default(),
        };
        assert!(manager.remember(&connection.tokens).await);
        // A refused connection uses the same reqwest is_connect classification as DNS failure.
        for (result, expected) in [
            (connection_failure().await, "service_unavailable"),
            (
                token_response(503, String::new()).await,
                "service_unavailable",
            ),
            (token_response(429, String::new()).await, "rate_limited"),
        ] {
            let old = Zeroizing::new(std::mem::take(&mut connection.tokens.refresh_token));
            assert_eq!(
                complete_refresh(&manager, &mut connection, old, result).await,
                Err(error(expected)),
            );
            assert!(connection.remembered);
            assert_eq!(connection.tokens.refresh_token, "r".repeat(43));
            let saved = manager.vault().call(None, true).await.unwrap().unwrap();
            let saved: Saved = serde_json::from_str(&saved).unwrap();
            assert_eq!(saved.refresh_token, "r".repeat(43));
        }
        // A fresh restore still reads the original credential, then persists the rotated one.
        let saved = manager.vault().call(None, true).await.unwrap().unwrap();
        let saved: Saved = serde_json::from_str(&saved).unwrap();
        let old = Zeroizing::new(saved.refresh_token.clone());
        let response = token_response(
            200,
            serde_json::json!({
                "access_token": "a".repeat(43), "refresh_token": "n".repeat(43),
                "account_id": saved.account_id, "expires_in": 900,
                "token_type": "Bearer", "scope": "library:read",
            })
            .to_string(),
        )
        .await;
        complete_refresh(&manager, &mut connection, old, response)
            .await
            .unwrap();
        assert_eq!(connection.tokens.refresh_token, "n".repeat(43));
        let saved = manager.vault().call(None, true).await.unwrap().unwrap();
        let saved: Saved = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved.refresh_token, "n".repeat(43));
        manager.vault().call(None, false).await.unwrap();
    }

    #[tokio::test]
    async fn remembered_restore_retries_after_an_outage_without_browser_login() {
        fn manager() -> ProviderConnection {
            ProviderConnection {
                vault: OnceLock::from(Vault::with_initializer(
                    mock_store,
                    "com.beambench.desktop-library.test.restore",
                )),
                ..Default::default()
            }
        }
        let first_process = manager();
        assert!(first_process.remember(&fixture_tokens('r')).await);
        drop(first_process);
        for status in [0, 503, 429] {
            let relaunched = manager();
            let saved = relaunched.vault().call(None, true).await.unwrap().unwrap();
            let saved: Saved = serde_json::from_str(&saved).unwrap();
            let response = if status == 0 {
                connection_failure().await
            } else {
                token_response(status, String::new()).await
            };
            let result = restored(&relaunched, 0, saved, response).await;
            assert_eq!(
                result.err().unwrap(),
                if status == 429 {
                    "rate_limited"
                } else {
                    "service_unavailable"
                },
            );
            let saved = relaunched.vault().call(None, true).await.unwrap().unwrap();
            let saved: Saved = serde_json::from_str(&saved).unwrap();
            assert_eq!(saved.refresh_token, "r".repeat(43));
        }
        let relaunched = manager();
        let saved = relaunched.vault().call(None, true).await.unwrap().unwrap();
        let saved: Saved = serde_json::from_str(&saved).unwrap();
        let response = token_response(
            200,
            serde_json::json!({
                "access_token": "a".repeat(43), "refresh_token": "n".repeat(43),
                "account_id": "disposable-test-account", "expires_in": 900,
                "token_type": "Bearer", "scope": "library:read",
            })
            .to_string(),
        )
        .await;
        let connection = restored(&relaunched, 0, saved, response).await.unwrap();
        assert!(connection.remembered);
        assert_eq!(connection.tokens.refresh_token, "n".repeat(43));
        let saved = relaunched.vault().call(None, true).await.unwrap().unwrap();
        let saved: Saved = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved.refresh_token, "n".repeat(43));
        relaunched.vault().call(None, false).await.unwrap();
    }

    #[tokio::test]
    async fn rejected_or_ambiguous_refresh_discards_the_credential() {
        let manager = ProviderConnection {
            vault: OnceLock::from(Vault::with_initializer(
                mock_store,
                "com.beambench.desktop-library.test.discard",
            )),
            ..Default::default()
        };
        for (status, body) in [(400, "{}"), (401, "{}"), (500, "{}"), (200, "{}"), (0, "")] {
            let mut connection = Connection {
                tokens: fixture_tokens('r'),
                expires: tokio::time::Instant::now(),
                remembered: true,
                wallet_unavailable: false,
                last_account: None,
                completed_inserts: Default::default(),
                inflight_inserts: Default::default(),
            };
            assert!(manager.remember(&connection.tokens).await);
            let old = Zeroizing::new(std::mem::take(&mut connection.tokens.refresh_token));
            let response = token_response(status, body.into()).await;
            assert_eq!(
                complete_refresh(&manager, &mut connection, old, response).await,
                Err(error("invalid_token")),
            );
            assert!(!connection.remembered);
            assert!(connection.tokens.refresh_token.is_empty());
            assert!(manager.vault().call(None, true).await.unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn credential_worker_round_trip_and_cancelled_write_cleanup() {
        let vault = Vault::with_initializer(mock_store, "com.beambench.desktop-library.test");
        vault
            .call(Some(Zeroizing::new("disposable-test-secret".into())), false)
            .await
            .unwrap();
        assert_eq!(
            vault
                .call(None, true)
                .await
                .unwrap()
                .as_deref()
                .map(|s| s.as_str()),
            Some("disposable-test-secret")
        );
        let old = vault.generation.load(Ordering::Acquire);
        vault.invalidate();
        let (reply, response) = oneshot::channel();
        vault
            .sender
            .try_send(VaultJob {
                generation: old,
                value: Some(Zeroizing::new("cancelled-secret".into())),
                read: false,
                reply,
            })
            .ok()
            .unwrap();
        assert_eq!(response.await.unwrap().unwrap_err(), "connection_changed");
        assert!(vault.call(None, true).await.unwrap().is_none());
    }

    #[test]
    fn loopback_binds_host_state_path_and_single_code() {
        let state = "s".repeat(43);
        let code = "c".repeat(43);
        let request = format!(
            "GET {CALLBACK}?state={state}&code={code} HTTP/1.1\r\nHost: 127.0.0.1:43210\r\n\r\n"
        );
        assert_eq!(
            callback_code(&request, 43210, &state, LibraryProvider::Craftgineer).unwrap(),
            code
        );
        assert!(callback_code(&request, 43210, &state, LibraryProvider::Printcutcarve).is_err());
        let pcc_request = request.replace(CALLBACK, callback(LibraryProvider::Printcutcarve));
        assert_eq!(
            callback_code(&pcc_request, 43210, &state, LibraryProvider::Printcutcarve).unwrap(),
            code
        );
        for bad in [
            request.replace(":43210", ":43211"),
            request.replace("state=", "state=x"),
            request.replace("&code=", "&code=a&code="),
            request.replace(CALLBACK, "/other"),
            request.replace("GET", "POST"),
        ] {
            assert!(callback_code(&bad, 43210, &state, LibraryProvider::Craftgineer).is_err());
        }
    }
}
