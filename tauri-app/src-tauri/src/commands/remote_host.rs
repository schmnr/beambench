//! Native-only remote credentials and transport. No bearer token is returned to
//! the webview, put in a URL, or sent through redirects / ambient HTTP proxies.
use super::connected_library::{Vault, install_credential_store};
use base64::{Engine, engine::general_purpose::STANDARD};
use beambench_service::{ServiceContext, ops::planning::SessionJobOptions};
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};
use tauri::State;
use tokio::sync::Mutex;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Deserialize, Serialize)]
struct Saved {
    url: String,
    token: String,
    client_id: String,
}
impl Drop for Saved {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}
#[derive(Default)]
pub struct RemoteHost {
    saved: Mutex<Option<Saved>>,
    vault: OnceLock<Vault>,
}
impl RemoteHost {
    fn vault(&self) -> &Vault {
        self.vault.get_or_init(|| {
            Vault::with_provider(
                install_credential_store,
                "com.beambench.remote-host",
                "host",
            )
        })
    }
    async fn connection(&self) -> Result<Saved, String> {
        let mut saved = self.saved.lock().await;
        if saved.is_none()
            && let Some(value) = self.vault().call(None, true).await.map_err(|_| {
                "Unlock the OS credential store to load this remote host".to_string()
            })?
        {
            let value: Saved = serde_json::from_str(&value)
                .map_err(|_| "Saved remote host is invalid. Configure it again.".to_string())?;
            validate_url(&value.url)?;
            *saved = Some(value);
        }
        saved
            .clone()
            .ok_or_else(|| "Configure a remote host first".into())
    }
}
fn validate_url(raw: &str) -> Result<String, String> {
    let url = Url::parse(raw.trim())
        .map_err(|_| "Enter an http:// or https:// host address".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("Use a host address with no credentials, path, query or fragment".into());
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}
fn transport() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| "Cannot create remote connection".into())
}
async fn request(
    saved: &Saved,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let mut request = transport()?
        .request(method, format!("{}{path}", saved.url))
        .bearer_auth(&saved.token)
        .header("x-beambench-client", &saved.client_id);
    if path == "/api/v1/remote/status" {
        request = request.timeout(Duration::from_secs(4));
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let mut response = request.send().await.map_err(|_| "Remote host unreachable. Its job may still be running. Reconnect to read its state; use the physical stop if needed.".to_string())?;
    let status = response.status();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Remote response interrupted. Reconnect to read the host state.".to_string())?
    {
        if bytes.len() + chunk.len() > 128 * 1024 * 1024 {
            return Err("Remote response exceeds 128 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "Remote authentication failed. Check the host token and address.".into(),
            _ => value["error"]["message"]
                .as_str()
                .or(value["message"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Remote host returned HTTP {status}")),
        });
    }
    Ok(value)
}
#[tauri::command]
pub async fn remote_host_settings(remote: State<'_, RemoteHost>) -> Result<Value, String> {
    match remote.connection().await {
        Ok(saved) => Ok(json!({"url":saved.url,"configured":true})),
        Err(e) if e == "Configure a remote host first" => Ok(json!({"url":"","configured":false})),
        Err(e) => Err(e),
    }
}
#[tauri::command]
pub async fn remote_host_configure(
    remote: State<'_, RemoteHost>,
    url: String,
    token: String,
) -> Result<Value, String> {
    let token = Zeroizing::new(token);
    let url = validate_url(&url)?;
    if token.len() < 32
        || !token.is_ascii()
        || token
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err("Use a host token with at least 32 ASCII characters and no whitespace".into());
    }
    let previous = remote.connection().await.ok();
    let client_id = previous
        .as_ref()
        .filter(|p| p.url == url)
        .map(|p| p.client_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let saved = Saved {
        url,
        token: token.to_string(),
        client_id,
    };
    let status = request(&saved, Method::GET, "/api/v1/remote/status", None).await?;
    if status["protocol_version"] != 1 {
        return Err(
            "Update Beam Bench on the remote host to use remote desktop connections".into(),
        );
    }
    // Verify the destination before persisting a credential. Never return its token.
    remote
        .vault()
        .call(
            Some(Zeroizing::new(
                serde_json::to_string(&saved).map_err(|e| e.to_string())?,
            )),
            false,
        )
        .await
        .map_err(|_| "Unlock the OS credential store to save this remote host".to_string())?;
    let result = json!({"url":saved.url,"configured":true});
    *remote.saved.lock().await = Some(saved);
    Ok(result)
}
#[tauri::command]
pub async fn remote_host_request(
    remote: State<'_, RemoteHost>,
    path: String,
    body: Option<Value>,
) -> Result<Value, String> {
    let method = match path.as_str() {
        "/api/v1/remote/status" | "/api/v1/remote/review" | "/api/v1/machine/ports" => Method::GET,
        "/api/v1/remote/claim"
        | "/api/v1/remote/release"
        | "/api/v1/remote/start"
        | "/api/v1/remote/frame"
        | "/api/v1/remote/connect"
        | "/api/v1/remote/origin"
        | "/api/v1/machine/home"
        | "/api/v1/machine/unlock"
        | "/api/v1/machine/emergency-stop"
        | "/api/v1/jobs/pause"
        | "/api/v1/jobs/resume"
        | "/api/v1/jobs/cancel" => Method::POST,
        _ => return Err("Unsupported remote operation".into()),
    };
    let saved = remote.connection().await?;
    request(&saved, method, &path, body).await
}
#[tauri::command]
pub async fn remote_host_prepare(
    remote: State<'_, RemoteHost>,
    ctx: State<'_, Arc<ServiceContext>>,
    profile_id: String,
    options: SessionJobOptions,
) -> Result<Value, String> {
    let saved = remote.connection().await?;
    let ctx = ctx.inner().clone();
    let archive = tokio::task::spawn_blocking(move || {
        let document = ctx
            .project
            .lock()
            .map_err(|_| "Cannot read local project")?
            .clone()
            .ok_or("Open a local project first")?;
        let bytes =
            beambench_project::save_project_to_bytes(&document).map_err(|e| e.to_string())?;
        if bytes.len() > beambench_api::routes::remote::MAX_ARCHIVE_BYTES {
            return Err("Remote project exceeds 64 MiB".into());
        }
        Ok::<_, String>(STANDARD.encode(bytes))
    })
    .await
    .map_err(|_| "Cannot prepare local project".to_string())??;
    request(
        &saved,
        Method::POST,
        "/api/v1/remote/prepare",
        Some(json!({"archive_base64":archive,"profile_id":profile_id,"options":options})),
    )
    .await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addresses_cannot_hide_credentials_or_change_request_paths() {
        assert_eq!(
            validate_url(" http://laser.local:5900/ ").unwrap(),
            "http://laser.local:5900"
        );
        for url in [
            "file:///etc/passwd",
            "http://user:secret@laser/",
            "http://laser/api",
            "http://laser/?token=abc",
            "http://laser/#token",
            "https://laser@evil/",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
    }
}
