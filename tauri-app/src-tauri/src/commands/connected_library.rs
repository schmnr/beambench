//! Craftgineer credentials and authenticated bytes stay in the native process.
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use beambench_common::connected_library::{
    LibraryAccountResponse, LibraryFile, LibraryListingResponse, LibraryMimeType,
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
use tauri::{Manager, State};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, oneshot},
};
use zeroize::{Zeroize, Zeroizing};

const ORIGIN: &str = "https://craftgineer.com";
const API: &str = "/api/integrations/desktop/v1";
const CALLBACK: &str = "/oauth/craftgineer/callback";
const MAX_FILE: usize = 25 * 1024 * 1024;
type Result<T> = std::result::Result<T, String>;
fn error(code: &str) -> String {
    code.to_owned()
}

#[derive(Deserialize, Serialize)]
struct Saved {
    account_id: String,
    refresh_token: String,
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
    tokens: Tokens,
    expires: tokio::time::Instant,
    remembered: bool,
    wallet_unavailable: bool,
    completed_inserts: std::collections::VecDeque<String>,
}
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    account: Option<LibraryAccountResponse>,
    remembered: bool,
    wallet_unavailable: bool,
}

// Exactly one lazy worker. A blocked wallet consumes at most this one thread;
// timeouts never start another. Generation checks remove late credential writes.
struct Vault {
    sender: std::sync::mpsc::SyncSender<VaultJob>,
    generation: Arc<AtomicU64>,
}
struct VaultJob {
    generation: u64,
    value: Option<Zeroizing<String>>,
    read: bool,
    reply: oneshot::Sender<Result<Option<Zeroizing<String>>>>,
}
fn install_credential_store() -> Result<()> {
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
    fn new() -> Self {
        Self::with_initializer(install_credential_store, "com.beambench.desktop-library")
    }
    fn with_initializer(initialize: fn() -> Result<()>, service: &'static str) -> Self {
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
                        let entry = keyring_core::Entry::new(service, "craftgineer")
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
    async fn call(
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
pub struct ConnectedLibrary {
    connection: Mutex<Option<Connection>>,
    generation: Arc<AtomicU64>,
    vault: OnceLock<Vault>,
}
impl ConnectedLibrary {
    fn vault(&self) -> &Vault {
        self.vault.get_or_init(Vault::new)
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
async fn token_request(fields: &[(&str, &str)]) -> std::result::Result<Tokens, TokenRequestError> {
    receive_tokens(
        client()
            .map_err(TokenRequestError::Retryable)?
            .post(format!("{ORIGIN}{API}/token"))
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
async fn account(tokens: &Tokens) -> Result<LibraryAccountResponse> {
    let response = client()?
        .get(format!("{ORIGIN}{API}/account"))
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|_| error("service_unavailable"))?;
    let account: LibraryAccountResponse =
        serde_json::from_slice(&bounded(response, 512 * 1024).await?)
            .map_err(|_| error("invalid_response"))?;
    account.validate().map_err(error)?;
    if account.account.id != tokens.account_id {
        return Err(error("library_account_mismatch"));
    }
    Ok(account)
}
async fn refreshed(manager: &ConnectedLibrary, connection: &mut Connection) -> Result<()> {
    if connection.expires > tokio::time::Instant::now() {
        return Ok(());
    }
    // Only a failure known to precede rotation leaves this token available for retry.
    if connection.tokens.refresh_token.is_empty() {
        return Err(error("invalid_token"));
    }
    let old_refresh = Zeroizing::new(std::mem::take(&mut connection.tokens.refresh_token));
    let result = token_request(&[
        ("client_id", "beambench-desktop"),
        ("grant_type", "refresh_token"),
        ("refresh_token", &old_refresh),
    ])
    .await;
    complete_refresh(manager, connection, old_refresh, result).await
}
async fn complete_refresh(
    manager: &ConnectedLibrary,
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

#[tauri::command]
pub async fn craftgineer_status(
    manager: State<'_, ConnectedLibrary>,
    restore: bool,
) -> Result<ConnectionStatus> {
    let generation = manager.generation.load(Ordering::Acquire);
    let mut guard = manager.connection.lock().await;
    let result = async {
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
            if let Some(saved) = saved {
                let saved: Saved =
                    serde_json::from_str(&saved).map_err(|_| error("invalid_token"))?;
                let tokens = token_request(&[
                    ("client_id", "beambench-desktop"),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &saved.refresh_token),
                ])
                .await
                .map_err(TokenRequestError::refresh_code)?;
                if tokens.account_id != saved.account_id {
                    return Err(error("library_account_mismatch"));
                }
                manager.check(generation)?;
                let remembered = manager.remember(&tokens).await;
                *guard = Some(Connection {
                    expires: tokio::time::Instant::now()
                        + Duration::from_secs(tokens.expires_in.saturating_sub(30)),
                    tokens,
                    remembered,
                    wallet_unavailable: !remembered,
                    completed_inserts: Default::default(),
                });
            }
        }
        let Some(connection) = guard.as_mut() else {
            return Ok(ConnectionStatus::default());
        };
        refreshed(&manager, connection).await?;
        let account = account(&connection.tokens).await?;
        manager.check(generation)?;
        Ok(ConnectionStatus {
            account: Some(account),
            remembered: connection.remembered,
            wallet_unavailable: connection.wallet_unavailable,
        })
    }
    .await;
    if restore && guard.is_none() && result.as_ref().is_err_and(|code| code == "invalid_token") {
        manager.vault().invalidate();
        let _ = manager.vault().call(None, false).await;
    }
    finish(&manager, &mut guard, result).await
}

async fn finish<T>(
    manager: &ConnectedLibrary,
    guard: &mut Option<Connection>,
    result: Result<T>,
) -> Result<T> {
    if guard.is_some()
        && result.as_ref().is_err_and(|code| {
            matches!(code.as_str(), "invalid_token" | "library_account_mismatch")
        })
    {
        *guard = None;
        manager.generation.fetch_add(1, Ordering::AcqRel);
        manager.vault().invalidate();
        let _ = manager.vault().call(None, false).await;
    }
    result
}

fn random_secret() -> String {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}
fn callback_code(request: &str, port: u16, expected_state: &str) -> Result<String> {
    let mut lines = request.split("\r\n");
    let line = lines.next().ok_or_else(|| error("invalid_callback"))?;
    let parts: Vec<_> = line.split(' ').collect();
    if parts.len() != 3
        || parts[0] != "GET"
        || parts[2] != "HTTP/1.1"
        || !parts[1].starts_with(&format!("{CALLBACK}?"))
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
    if values("error") == ["access_denied"] && values("code").is_empty() {
        return Err(error("access_denied"));
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
pub async fn craftgineer_connect(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    manager: State<'_, ConnectedLibrary>,
    remember: bool,
) -> Result<ConnectionStatus> {
    let generation = manager.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let mut guard = manager.connection.lock().await;
    if guard.is_some() {
        return Err(error("already_connected"));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| error("loopback_unavailable"))?;
    let port = listener
        .local_addr()
        .map_err(|_| error("loopback_unavailable"))?
        .port();
    let redirect = format!("http://127.0.0.1:{port}{CALLBACK}");
    let verifier = Zeroizing::new(random_secret());
    let state = random_secret();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = reqwest::Url::parse(&format!("{ORIGIN}/integrations/desktop/authorize"))
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
            if window.is_visible().is_err() { return Err(error("connection_changed")); }
            let accepted = tokio::time::timeout(Duration::from_millis(250), listener.accept()).await;
            let Ok(Ok((mut stream, _))) = accepted else { continue; };
            let mut bytes = Vec::new();
            let read = tokio::time::timeout(Duration::from_secs(2), async {
                while bytes.len() < 8192 && !bytes.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    if stream.read(&mut byte).await.unwrap_or(0) == 0 { break; }
                    bytes.push(byte[0]);
                }
            }).await;
            if read.is_err() || !bytes.ends_with(b"\r\n\r\n") { continue; }
            let result = callback_code(&String::from_utf8_lossy(&bytes), port, &state);
            // No secrets or account data in the browser response; the app reports success.
            let response = b"HTTP/1.1 204 No Content\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\n\r\n";
            let _ = tokio::time::timeout(Duration::from_secs(1), stream.write_all(response)).await;
            match result { Ok(code) => return Ok(code), Err(code) if code == "access_denied" => return Err(code), Err(_) => continue }
        }
    }).await.map_err(|_| error("connection_timeout"))??;
    manager.check(generation)?;
    let tokens = token_request(&[
        ("client_id", "beambench-desktop"),
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("code_verifier", &verifier),
        ("redirect_uri", &redirect),
    ])
    .await
    .map_err(TokenRequestError::code)?;
    let result = async {
        let account = account(&tokens).await?;
        manager.check(generation)?;
        if window.is_visible().is_err() {
            return Err(error("connection_changed"));
        }
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
        Ok::<_, String>((account, remembered))
    }
    .await;
    let (account, remembered) = match result {
        Ok(value) => value,
        Err(code) => {
            let _ = revoke_remote(&tokens.refresh_token).await;
            return Err(code);
        }
    };
    *guard = Some(Connection {
        expires: tokio::time::Instant::now()
            + Duration::from_secs(tokens.expires_in.saturating_sub(30)),
        tokens,
        remembered,
        wallet_unavailable: remember && !remembered,
        completed_inserts: Default::default(),
    });
    Ok(ConnectionStatus {
        account: Some(account),
        remembered,
        wallet_unavailable: remember && !remembered,
    })
}
#[tauri::command]
pub async fn craftgineer_cancel(manager: State<'_, ConnectedLibrary>) -> Result<()> {
    craftgineer_disconnect(manager).await
}
async fn revoke_remote(token: &str) -> bool {
    tokio::time::timeout(Duration::from_secs(4), async {
        client()?
            .post(format!("{ORIGIN}{API}/revoke"))
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
pub async fn craftgineer_disconnect(manager: State<'_, ConnectedLibrary>) -> Result<()> {
    manager.generation.fetch_add(1, Ordering::AcqRel);
    if let Some(vault) = manager.vault.get() {
        vault.invalidate();
    }
    let mut guard = manager.connection.lock().await;
    let connection = guard.take();
    let removed = manager.vault().call(None, false).await.is_ok();
    let revoked = if let Some(connection) = connection {
        revoke_remote(&connection.tokens.refresh_token).await
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
pub async fn craftgineer_files(
    manager: State<'_, ConnectedLibrary>,
    q: String,
    cursor: Option<String>,
) -> Result<LibraryListingResponse> {
    if q.chars().count() > 120 || cursor.as_ref().is_some_and(|v| v.len() > 2048) {
        return Err(error("invalid_request"));
    }
    let generation = manager.generation.load(Ordering::Acquire);
    let mut guard = manager.connection.lock().await;
    let result = async {
        let connection = guard.as_mut().ok_or_else(|| error("invalid_token"))?;
        refreshed(&manager, connection).await?;
        let mut url = reqwest::Url::parse(&format!("{ORIGIN}{API}/files"))
            .map_err(|_| error("invalid_request"))?;
        url.query_pairs_mut()
            .append_pair("q", &q)
            .append_pair("limit", "24");
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("cursor", &cursor);
        }
        let response = client()?
            .get(url)
            .bearer_auth(&connection.tokens.access_token)
            .send()
            .await
            .map_err(|_| error("service_unavailable"))?;
        let listing: LibraryListingResponse =
            serde_json::from_slice(&bounded(response, 512 * 1024).await?)
                .map_err(|_| error("invalid_response"))?;
        listing
            .validate_for(&connection.tokens.account_id)
            .map_err(error)?;
        manager.check(generation)?;
        Ok(listing)
    }
    .await;
    finish(&manager, &mut guard, result).await
}
async fn file_bytes(
    connection: &Connection,
    file: &LibraryFile,
    thumbnail: bool,
) -> Result<Vec<u8>> {
    // Validate even when the renderer supplied this metadata.
    LibraryListingResponse {
        schema_version: 1,
        provider: beambench_common::connected_library::LibraryProvider::Craftgineer,
        account_id: connection.tokens.account_id.clone(),
        files: vec![file.clone()],
        next_cursor: None,
    }
    .validate_for(&connection.tokens.account_id)
    .map_err(error)?;
    let suffix = if thumbnail { "thumbnail" } else { "content" };
    let response = client()?
        .get(format!("{ORIGIN}{API}/files/{}/{suffix}", file.id))
        .bearer_auth(&connection.tokens.access_token)
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
pub async fn craftgineer_thumbnail(
    manager: State<'_, ConnectedLibrary>,
    file: LibraryFile,
) -> Result<String> {
    let generation = manager.generation.load(Ordering::Acquire);
    let mut guard = manager.connection.lock().await;
    let result = async {
        let connection = guard.as_mut().ok_or_else(|| error("invalid_token"))?;
        refreshed(&manager, connection).await?;
        let bytes = file_bytes(connection, &file, true).await?;
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
    finish(&manager, &mut guard, result).await
}
#[tauri::command]
pub fn craftgineer_prepare_insert(
    svc: State<'_, Arc<ServiceContext>>,
    project_id: String,
) -> Result<u64> {
    imports::connected_library_document_epoch(&svc, &project_id).map_err(Into::into)
}
#[tauri::command]
pub async fn craftgineer_insert(
    window: tauri::WebviewWindow,
    manager: State<'_, ConnectedLibrary>,
    svc: State<'_, Arc<ServiceContext>>,
    file: LibraryFile,
    project_id: String,
    expected_epoch: u64,
    request_id: String,
    layer_id: String,
    create_layer: Option<beambench_service::ops::project::AddObjectLayerInput>,
) -> Result<Vec<ProjectObject>> {
    if request_id.len() != 36 || uuid::Uuid::parse_str(&request_id).is_err() {
        return Err(error("invalid_request"));
    }
    let epoch = expected_epoch;
    let generation = manager.generation.load(Ordering::Acquire);
    let mut guard = manager.connection.lock().await;
    let result = async {
        let connection = guard.as_mut().ok_or_else(|| error("invalid_token"))?;
        if connection.completed_inserts.contains(&request_id) {
            return Ok(Vec::new());
        }
        refreshed(&manager, connection).await?;
        let bytes = file_bytes(connection, &file, false).await?;
        manager.check(generation)?;
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
        connection.completed_inserts.push_back(request_id);
        if connection.completed_inserts.len() > 128 {
            connection.completed_inserts.pop_front();
        }
        Ok(objects)
    }
    .await;
    finish(&manager, &mut guard, result).await
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

    #[tokio::test]
    async fn remembered_refresh_survives_offline_and_pre_rotation_rejections() {
        let manager = ConnectedLibrary {
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
            completed_inserts: Default::default(),
        };
        assert!(manager.remember(&connection.tokens).await);
        // A refused connection uses the same reqwest is_connect classification as DNS failure.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let offline = receive_tokens(
            reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .post(format!("http://{address}/token")),
        )
        .await;
        for (result, expected) in [
            (offline, "service_unavailable"),
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
    async fn rejected_or_ambiguous_refresh_discards_the_credential() {
        let manager = ConnectedLibrary {
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
                completed_inserts: Default::default(),
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
        assert_eq!(callback_code(&request, 43210, &state).unwrap(), code);
        for bad in [
            request.replace(":43210", ":43211"),
            request.replace("state=", "state=x"),
            request.replace("&code=", "&code=a&code="),
            request.replace(CALLBACK, "/other"),
            request.replace("GET", "POST"),
        ] {
            assert!(callback_code(&bad, 43210, &state).is_err());
        }
    }
}
