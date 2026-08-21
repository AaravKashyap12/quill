use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;

const SERVICE: &str = "com.quill.desktop.cloud-providers";
const GROQ_STATUS_URL: &str = "https://api.groq.com/openai/v1/models/whisper-large-v3";
pub(crate) const GEMINI_MODEL: &str = "gemini-3.1-flash-lite";
static CLOUD_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Shared only by internet providers. Localhost clients deliberately remain
/// separate because they disable proxy discovery, while cloud requests must
/// continue to respect a user's corporate or VPN proxy configuration.
pub(crate) fn cloud_client() -> Result<reqwest::Client, String> {
    if let Some(client) = CLOUD_CLIENT.get() {
        return Ok(client.clone());
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .pool_idle_timeout(Duration::from_secs(120))
        .tcp_keepalive(Duration::from_secs(60))
        .build()
        .map_err(|_| "Could not prepare the cloud connection.".to_owned())?;
    let _ = CLOUD_CLIENT.set(client);
    Ok(CLOUD_CLIENT
        .get()
        .expect("cloud client was initialized")
        .clone())
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CloudProvider {
    Groq,
    Gemini,
}

impl CloudProvider {
    fn account(self) -> &'static str {
        match self {
            Self::Groq => "groq-api-key",
            Self::Gemini => "gemini-api-key",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKeyStatus {
    provider: CloudProvider,
    configured: bool,
    status: &'static str,
    message: Option<String>,
}

#[cfg(any(windows, target_os = "macos"))]
fn entry(provider: CloudProvider) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, provider.account())
        .map_err(|_| "The operating system credential store is unavailable.".to_owned())
}

#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn get_key(provider: CloudProvider) -> Result<Option<String>, String> {
    match entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(
            "The saved API key could not be read from the operating system credential store."
                .to_owned(),
        ),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn get_key(_provider: CloudProvider) -> Result<Option<String>, String> {
    Err("Secure API key storage is supported only on Windows and macOS.".to_owned())
}

#[tauri::command]
pub fn set_provider_key(provider: CloudProvider, key: String) -> Result<ProviderKeyStatus, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Enter an API key before saving.".to_owned());
    }
    #[cfg(any(windows, target_os = "macos"))]
    entry(provider)?.set_password(key).map_err(|_| {
        "The API key could not be saved to the operating system credential store.".to_owned()
    })?;
    #[cfg(not(any(windows, target_os = "macos")))]
    return Err("Secure API key storage is supported only on Windows and macOS.".to_owned());

    Ok(status(provider, true, "configured", None))
}

#[tauri::command]
pub fn delete_provider_key(provider: CloudProvider) -> Result<ProviderKeyStatus, String> {
    #[cfg(any(windows, target_os = "macos"))]
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(_) => {
            return Err(
                "The API key could not be removed from the operating system credential store."
                    .to_owned(),
            )
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    return Err("Secure API key storage is supported only on Windows and macOS.".to_owned());

    Ok(status(provider, false, "missing", None))
}

#[tauri::command]
pub fn get_provider_key_status(provider: CloudProvider) -> Result<ProviderKeyStatus, String> {
    let configured = get_key(provider)?.is_some();
    Ok(if configured {
        status(provider, true, "configured", None)
    } else {
        status(provider, false, "missing", None)
    })
}

#[tauri::command]
pub async fn test_provider_connection(
    provider: CloudProvider,
) -> Result<ProviderKeyStatus, String> {
    let key = get_key(provider)?.ok_or_else(|| "Save an API key first.".to_owned())?;
    let client = cloud_client()?;
    let request = match provider {
        CloudProvider::Groq => client.get(GROQ_STATUS_URL).bearer_auth(&key),
        CloudProvider::Gemini => client
            .get(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{GEMINI_MODEL}"
            ))
            .header("x-goog-api-key", &key),
    };
    let response = request
        .timeout(Duration::from_secs(12))
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                "The provider connection timed out.".to_owned()
            } else {
                "The provider could not be reached. Check your connection and try again.".to_owned()
            }
        })?;
    if response.status().is_success() {
        return Ok(status(provider, true, "connected", None));
    }
    let code = response.status().as_u16();
    let message = match code {
        401 | 403 => "The API key was rejected.",
        429 => "The provider quota is currently exhausted.",
        _ if code >= 500 => "The provider is temporarily unavailable.",
        _ => "The provider connection test failed.",
    };
    Ok(status(provider, true, "error", Some(message.to_owned())))
}

fn status(
    provider: CloudProvider,
    configured: bool,
    state: &'static str,
    message: Option<String>,
) -> ProviderKeyStatus {
    ProviderKeyStatus {
        provider,
        configured,
        status: state,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_accounts_are_separate() {
        assert_ne!(
            CloudProvider::Groq.account(),
            CloudProvider::Gemini.account()
        );
    }

    #[test]
    fn status_never_serializes_a_key() {
        let json =
            serde_json::to_string(&status(CloudProvider::Groq, true, "configured", None)).unwrap();
        assert!(!json.contains("api-key"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn cloud_client_is_reused() {
        let _first = cloud_client().unwrap();
        assert!(CLOUD_CLIENT.get().is_some());
        let _second = cloud_client().unwrap();
    }

    #[test]
    fn groq_connection_test_targets_the_transcription_model() {
        assert!(GROQ_STATUS_URL.ends_with("/models/whisper-large-v3"));
    }

    /// Run this and `credential_probe_read_after_process_restart_and_delete`
    /// as separate cargo invocations to exercise the real Windows vault across
    /// process lifetimes. The probe account is isolated from Quill's provider
    /// accounts and the value is deliberately non-secret.
    #[cfg(windows)]
    #[test]
    #[ignore = "writes a temporary Windows Credential Manager entry"]
    fn credential_probe_store_for_process_restart() {
        let entry = keyring::Entry::new(SERVICE, "quill-credential-runtime-probe").unwrap();
        let _ = entry.delete_credential();
        entry.set_password("non-secret-runtime-probe").unwrap();
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "reads and deletes the temporary Windows Credential Manager entry"]
    fn credential_probe_read_after_process_restart_and_delete() {
        let entry = keyring::Entry::new(SERVICE, "quill-credential-runtime-probe").unwrap();
        let value = entry.get_password();
        let deleted = entry.delete_credential();
        assert_eq!(value.unwrap(), "non-secret-runtime-probe");
        deleted.unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
    }
}
