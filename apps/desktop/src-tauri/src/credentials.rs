use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const SERVICE: &str = "com.quill.desktop.cloud-providers";
const GROQ_STATUS_URL: &str = "https://api.groq.com/openai/v1/models/whisper-large-v3";
pub(crate) const GEMINI_MODEL: &str = "gemini-3.1-flash-lite";
static CLOUD_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static CREDENTIALS: OnceLock<CredentialCache<OsCredentialStore>> = OnceLock::new();

type KeyResult = Result<Option<String>, String>;

trait CredentialStore: Send + Sync + 'static {
    fn read(&self, provider: CloudProvider) -> KeyResult;
    fn save(&self, provider: CloudProvider, key: &str) -> Result<(), String>;
    fn delete(&self, provider: CloudProvider) -> Result<(), String>;
}

// Cache successes, absence and failures for this process. In particular, a
// denied Keychain dialog must not be reopened by the runtime's retry loop.
// Keys remain ordinary Strings, like request-local keys, but now live until
// replacement, deletion or process exit. Never Debug/serialize this cache.
struct CredentialCache<S> {
    store: Arc<S>,
    states: [Arc<tokio::sync::Mutex<Option<KeyResult>>>; 2],
}

impl<S: CredentialStore> CredentialCache<S> {
    fn new(store: S) -> Self {
        Self {
            store: Arc::new(store),
            states: std::array::from_fn(|_| Arc::new(tokio::sync::Mutex::new(None))),
        }
    }

    async fn state(
        &self,
        provider: CloudProvider,
    ) -> tokio::sync::OwnedMutexGuard<Option<KeyResult>> {
        let index = match provider {
            CloudProvider::Groq => 0,
            CloudProvider::Gemini => 1,
        };
        self.states[index].clone().lock_owned().await
    }

    async fn read(&self, provider: CloudProvider, retry_unavailable: bool) -> KeyResult {
        let state = self.state(provider).await;
        if let Some(result) = state.as_ref() {
            // Test retries a denied/missing key. A usable key (including one
            // just saved) never needs another OS read for a connection test.
            if !retry_unavailable || matches!(result, Ok(Some(_))) {
                return result.clone();
            }
        }
        self.run(state, move |store| store.read(provider)).await
    }

    async fn status(
        &self,
        provider: CloudProvider,
        request_access: bool,
    ) -> Result<ProviderKeyStatus, String> {
        let configured = if request_access {
            // Only the selected speech provider or an explicit action needs
            // credential access. This is not permission to retry a denial.
            self.read(provider, false).await?.is_some()
        } else {
            let index = match provider {
                CloudProvider::Groq => 0,
                CloudProvider::Gemini => 1,
            };
            // Status rendering must neither open Keychain nor wait behind a
            // dialog already open for another operation. Project metadata
            // under the lock without copying the secret into the UI result.
            let state = self.states[index].try_lock();
            match state.as_deref().ok().and_then(Option::as_ref) {
                Some(Ok(key)) => key.is_some(),
                Some(Err(error)) => return Err(error.clone()),
                None => return Ok(status(provider, false, "unchecked", Some(
                    "Saved key access has not been checked. It is requested only when this provider is used or you choose Check saved key.".to_owned()
                ))),
            }
        };
        Ok(status(
            provider,
            configured,
            if configured { "configured" } else { "missing" },
            None,
        ))
    }

    async fn save(&self, provider: CloudProvider, key: String) -> KeyResult {
        let key = key.trim().to_owned();
        if key.is_empty() {
            return Err("Enter an API key before saving.".to_owned());
        }
        let state = self.state(provider).await;
        self.run(state, move |store| {
            store.save(provider, &key)?;
            Ok(Some(key))
        })
        .await
    }

    async fn delete(&self, provider: CloudProvider) -> KeyResult {
        let state = self.state(provider).await;
        self.run(state, move |store| {
            store.delete(provider)?;
            Ok(None)
        })
        .await
    }

    async fn run(
        &self,
        mut state: tokio::sync::OwnedMutexGuard<Option<KeyResult>>,
        operation: impl FnOnce(&S) -> KeyResult + Send + 'static,
    ) -> KeyResult {
        // Fail closed even if the blocking task panics. The task owns the
        // lock: cancelling its async caller cannot start a second OS dialog
        // or let an older read overwrite a later save/delete.
        *state = Some(Err("The operating system credential store task could not complete. Use Test in Voice settings to retry.".to_owned()));
        let store = self.store.clone();
        credential_operation(move || {
            let result = operation(&store);
            *state = Some(result.clone());
            result
        })
        .await
    }
}

fn credentials() -> &'static CredentialCache<OsCredentialStore> {
    CREDENTIALS.get_or_init(|| CredentialCache::new(OsCredentialStore))
}

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
fn get_key_blocking(provider: CloudProvider) -> Result<Option<String>, String> {
    match entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(
            "The saved API key could not be read from the operating system credential store. Use Test in Voice settings to retry."
                .to_owned(),
        ),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn get_key_blocking(_provider: CloudProvider) -> Result<Option<String>, String> {
    Err("Secure API key storage is supported only on Windows and macOS.".to_owned())
}

// Credential APIs can wait for an OS permission dialog. Keep those waits off
// both the UI thread and async executor workers.
async fn credential_operation<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| "The operating system credential store task could not complete.".to_owned())?
}

struct OsCredentialStore;

impl CredentialStore for OsCredentialStore {
    fn read(&self, provider: CloudProvider) -> KeyResult {
        get_key_blocking(provider)
    }

    fn save(&self, provider: CloudProvider, key: &str) -> Result<(), String> {
        #[cfg(any(windows, target_os = "macos"))]
        {
            entry(provider)?.set_password(key).map_err(|_| {
                "The API key could not be saved to the operating system credential store."
                    .to_owned()
            })
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (provider, key);
            Err("Secure API key storage is supported only on Windows and macOS.".to_owned())
        }
    }

    fn delete(&self, provider: CloudProvider) -> Result<(), String> {
        #[cfg(any(windows, target_os = "macos"))]
        {
            match entry(provider)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(
                    "The API key could not be removed from the operating system credential store."
                        .to_owned(),
                ),
            }
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = provider;
            Err("Secure API key storage is supported only on Windows and macOS.".to_owned())
        }
    }
}

pub(crate) async fn get_key(provider: CloudProvider) -> KeyResult {
    credentials().read(provider, false).await
}

#[tauri::command]
pub async fn set_provider_key(
    provider: CloudProvider,
    key: String,
) -> Result<ProviderKeyStatus, String> {
    credentials().save(provider, key).await?;
    Ok(status(provider, true, "configured", None))
}

#[tauri::command]
pub async fn delete_provider_key(provider: CloudProvider) -> Result<ProviderKeyStatus, String> {
    credentials().delete(provider).await?;
    Ok(status(provider, false, "missing", None))
}

#[tauri::command]
pub async fn get_provider_key_status(
    provider: CloudProvider,
    request_access: Option<bool>,
) -> Result<ProviderKeyStatus, String> {
    credentials()
        .status(provider, request_access.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn test_provider_connection(
    provider: CloudProvider,
    retry_access: Option<bool>,
) -> Result<ProviderKeyStatus, String> {
    // Bootstrap also tests connections. Only an explicit user retry may
    // reopen a previously denied/missing credential; omitted means passive.
    let key = credentials()
        .read(provider, retry_access.unwrap_or(false))
        .await?
        .ok_or_else(|| "Save an API key first.".to_owned())?;
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

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    // Only this injected store is used by cache tests; no vault access.
    struct FakeStore {
        value: std::sync::Mutex<KeyResult>,
        reads: AtomicUsize,
        writes: AtomicUsize,
        fail_write: AtomicBool,
        read_gate: std::sync::Mutex<
            Option<(
                tokio::sync::oneshot::Sender<()>,
                std::sync::mpsc::Receiver<()>,
            )>,
        >,
    }

    impl FakeStore {
        fn new(value: KeyResult) -> Self {
            Self {
                value: std::sync::Mutex::new(value),
                reads: AtomicUsize::new(0),
                writes: AtomicUsize::new(0),
                fail_write: AtomicBool::new(false),
                read_gate: std::sync::Mutex::new(None),
            }
        }
    }

    impl CredentialStore for FakeStore {
        fn read(&self, _provider: CloudProvider) -> KeyResult {
            self.reads.fetch_add(1, Ordering::SeqCst);
            let result = self.value.lock().unwrap().clone();
            let gate = self.read_gate.lock().unwrap().take();
            if let Some((entered, release)) = gate {
                entered.send(()).unwrap();
                release
                    .recv_timeout(Duration::from_secs(2))
                    .expect("async executor must remain responsive during OS wait");
            }
            result
        }

        fn save(&self, _provider: CloudProvider, key: &str) -> Result<(), String> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            if self.fail_write.load(Ordering::SeqCst) {
                return Err("fixture write denied".to_owned());
            }
            *self.value.lock().unwrap() = Ok(Some(key.to_owned()));
            Ok(())
        }

        fn delete(&self, _provider: CloudProvider) -> Result<(), String> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            if self.fail_write.load(Ordering::SeqCst) {
                return Err("fixture delete denied".to_owned());
            }
            *self.value.lock().unwrap() = Ok(None);
            Ok(())
        }
    }

    fn gated_cache() -> (
        Arc<CredentialCache<FakeStore>>,
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let store = FakeStore::new(Ok(Some("fixture old key".to_owned())));
        *store.read_gate.lock().unwrap() = Some((entered_tx, release_rx));
        (
            Arc::new(CredentialCache::new(store)),
            entered_rx,
            release_tx,
        )
    }

    #[tokio::test]
    async fn passive_status_never_opens_either_provider_store() {
        let cache = CredentialCache::new(FakeStore::new(Ok(Some("fixture key".to_owned()))));
        for provider in [CloudProvider::Groq, CloudProvider::Gemini] {
            for _ in 0..3 {
                let result = cache.status(provider, false).await.unwrap();
                assert_eq!(result.status, "unchecked");
                assert!(!result.configured);
            }
        }
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 0);
        assert!(
            cache
                .status(CloudProvider::Groq, true)
                .await
                .unwrap()
                .configured
        );
        assert!(
            cache
                .status(CloudProvider::Groq, false)
                .await
                .unwrap()
                .configured
        );
        assert_eq!(
            cache
                .status(CloudProvider::Gemini, false)
                .await
                .unwrap()
                .status,
            "unchecked"
        );
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn passive_status_does_not_wait_on_an_existing_os_prompt() {
        let (cache, entered, release) = gated_cache();
        let reader_cache = cache.clone();
        let reader =
            tokio::spawn(async move { reader_cache.read(CloudProvider::Groq, false).await });
        entered.await.unwrap();
        let observed = tokio::time::timeout(
            Duration::from_millis(50),
            cache.status(CloudProvider::Groq, false),
        )
        .await;
        release.send(()).unwrap();
        reader.await.unwrap().unwrap();
        assert_eq!(
            observed
                .expect("passive status must remain responsive")
                .unwrap()
                .status,
            "unchecked"
        );
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_reads_share_one_blocking_store_read() {
        let (cache, entered, release) = gated_cache();
        let mut readers = Vec::new();
        for _ in 0..12 {
            let cache = cache.clone();
            readers.push(tokio::spawn(async move {
                cache.read(CloudProvider::Groq, false).await
            }));
        }
        entered.await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 1);
        release.send(()).unwrap();
        for reader in readers {
            assert_eq!(
                reader.await.unwrap(),
                Ok(Some("fixture old key".to_owned()))
            );
        }
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_reads_stay_cached_until_a_deliberate_retry() {
        let cache = CredentialCache::new(FakeStore::new(Err("fixture denied".to_owned())));
        for _ in 0..5 {
            assert_eq!(
                cache.read(CloudProvider::Groq, false).await,
                Err("fixture denied".to_owned())
            );
            assert!(cache.status(CloudProvider::Groq, false).await.is_err());
            assert!(cache.status(CloudProvider::Groq, true).await.is_err());
        }
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 1);
        assert!(cache.read(CloudProvider::Groq, true).await.is_err());
        assert!(cache.read(CloudProvider::Groq, false).await.is_err());
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 2);
        *cache.store.value.lock().unwrap() = Ok(Some("fixture recovered key".to_owned()));
        assert_eq!(
            cache.read(CloudProvider::Groq, true).await,
            Ok(Some("fixture recovered key".to_owned()))
        );
        assert!(cache
            .read(CloudProvider::Groq, true)
            .await
            .unwrap()
            .is_some());
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn missing_keys_are_cached_and_provider_caches_are_independent() {
        let cache = CredentialCache::new(FakeStore::new(Ok(None)));
        assert_eq!(cache.read(CloudProvider::Groq, false).await, Ok(None));
        *cache.store.value.lock().unwrap() = Ok(Some("fixture external key".to_owned()));
        assert_eq!(
            cache
                .status(CloudProvider::Groq, true)
                .await
                .unwrap()
                .status,
            "missing"
        );
        assert_eq!(cache.read(CloudProvider::Groq, false).await, Ok(None));
        assert!(cache
            .read(CloudProvider::Gemini, false)
            .await
            .unwrap()
            .is_some());
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 2);
        assert!(cache
            .read(CloudProvider::Groq, true)
            .await
            .unwrap()
            .is_some());
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_read_still_serializes_save_and_delete() {
        let (cache, entered, release) = gated_cache();
        let reader_cache = cache.clone();
        let reader =
            tokio::spawn(async move { reader_cache.read(CloudProvider::Groq, false).await });
        entered.await.unwrap();
        reader.abort();
        assert!(reader.await.unwrap_err().is_cancelled());
        let writer_cache = cache.clone();
        let writer = tokio::spawn(async move {
            writer_cache
                .save(CloudProvider::Groq, "  fixture new key  ".to_owned())
                .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(cache.store.writes.load(Ordering::SeqCst), 0);
        release.send(()).unwrap();
        assert_eq!(
            writer.await.unwrap(),
            Ok(Some("fixture new key".to_owned()))
        );
        // Both the normal path and Save -> Test must use the replacement.
        for retry in [false, true] {
            assert_eq!(
                cache.read(CloudProvider::Groq, retry).await,
                Ok(Some("fixture new key".to_owned()))
            );
        }
        assert_eq!(cache.delete(CloudProvider::Groq).await, Ok(None));
        assert_eq!(cache.read(CloudProvider::Groq, false).await, Ok(None));
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 1);
        assert_eq!(cache.store.writes.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failed_mutations_invalidate_cached_keys_without_automatic_reads() {
        let cache = CredentialCache::new(FakeStore::new(Ok(Some("fixture old key".to_owned()))));
        cache.read(CloudProvider::Groq, false).await.unwrap();
        cache.store.fail_write.store(true, Ordering::SeqCst);
        assert!(cache
            .save(CloudProvider::Groq, "fixture replacement".to_owned())
            .await
            .is_err());
        assert_eq!(
            cache.read(CloudProvider::Groq, false).await,
            Err("fixture write denied".to_owned())
        );
        assert!(cache.delete(CloudProvider::Groq).await.is_err());
        assert_eq!(
            cache.read(CloudProvider::Groq, false).await,
            Err("fixture delete denied".to_owned())
        );
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 1);
        cache.store.fail_write.store(false, Ordering::SeqCst);
        cache
            .save(CloudProvider::Groq, "fixture recovered".to_owned())
            .await
            .unwrap();
        assert_eq!(
            cache.read(CloudProvider::Groq, false).await,
            Ok(Some("fixture recovered".to_owned()))
        );
    }

    #[tokio::test]
    async fn panicked_store_task_does_not_trigger_an_automatic_retry() {
        let cache = CredentialCache::new(FakeStore::new(Ok(None)));
        let state = cache.state(CloudProvider::Groq).await;
        assert!(cache
            .run(state, |_| panic!("fixture store panic"))
            .await
            .is_err());
        assert!(cache.read(CloudProvider::Groq, false).await.is_err());
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 0);
        assert_eq!(cache.read(CloudProvider::Groq, true).await, Ok(None));
        assert_eq!(cache.store.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn credential_wait_does_not_block_the_async_executor() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let operation = credential_operation(move || {
            entered_tx.send(()).unwrap();
            // Model a modal OS-store wait, bounded so a regression cannot hang.
            release_rx
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| "executor could not release credential wait".to_owned())?;
            Ok("completed")
        });
        let responsive_task = async move {
            entered_rx.await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = release_tx.send(());
        };
        let (result, ()) = tokio::join!(operation, responsive_task);
        assert_eq!(result, Ok("completed"));
    }

    #[tokio::test]
    async fn credential_operation_preserves_store_errors() {
        let result =
            credential_operation(|| Err::<(), _>("store unavailable fixture".to_owned())).await;
        assert_eq!(result, Err("store unavailable fixture".to_owned()));
    }

    #[tokio::test]
    async fn blank_key_is_rejected_before_accessing_the_store() {
        assert_eq!(
            set_provider_key(CloudProvider::Groq, "  ".to_owned())
                .await
                .unwrap_err(),
            "Enter an API key before saving."
        );
    }

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
