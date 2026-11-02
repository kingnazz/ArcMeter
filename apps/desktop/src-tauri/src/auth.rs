use chrono::{DateTime, Duration, Utc};
use keyring::Entry;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tokio::sync::Mutex;

const KEYRING_SERVICE: &str = "com.arctechone.arcmeter";
const KEYRING_USER: &str = "supabase-session";
static SESSIONS: SessionManager<NativeSessionStore> = SessionManager::new(NativeSessionStore);

#[derive(Debug, Error)]
pub enum AuthError {
    #[error(
        "Supabase is not configured. Add the client-safe project URL and publishable key before signing in."
    )]
    NotConfigured,
    #[error("Invalid email or password")]
    InvalidCredentials,
    #[error("Your ArcMeter session expired. Sign in again to resume cloud sync.")]
    SessionExpired,
    #[error("Authentication service error: {0}")]
    Remote(String),
    #[error("Secure credential storage error: {0}")]
    Credential(String),
    #[error("Stored session is invalid")]
    InvalidSession,
}

#[derive(Debug, Clone)]
pub struct SupabaseConfig {
    pub url: String,
    pub client_key: String,
}

impl SupabaseConfig {
    pub fn load() -> Result<Self, AuthError> {
        let url = std::env::var("ARCMETER_SUPABASE_URL")
            .ok()
            .or_else(|| option_env!("ARCMETER_SUPABASE_URL").map(ToOwned::to_owned))
            .or_else(|| option_env!("VITE_SUPABASE_URL").map(ToOwned::to_owned))
            .filter(|value| value.starts_with("https://") && !value.contains("your-project"));
        let client_key = std::env::var("ARCMETER_SUPABASE_ANON_KEY")
            .ok()
            .or_else(|| option_env!("ARCMETER_SUPABASE_ANON_KEY").map(ToOwned::to_owned))
            .or_else(|| option_env!("VITE_SUPABASE_ANON_KEY").map(ToOwned::to_owned))
            .filter(|value| !value.contains("your-client-safe"));
        match (url, client_key) {
            (Some(url), Some(client_key)) => Ok(Self {
                url: url.trim_end_matches('/').into(),
                client_key,
            }),
            _ => Err(AuthError::NotConfigured),
        }
    }

    pub fn is_configured() -> bool {
        Self::load().is_ok()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub user_id: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub configured: bool,
    pub signed_in: bool,
    pub email: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    user: AuthUser,
}

#[derive(Debug, Deserialize)]
struct AuthUser {
    id: String,
    email: Option<String>,
}

pub fn status() -> AuthStatus {
    SESSIONS.status(SupabaseConfig::is_configured())
}

pub async fn sign_in(email: &str, password: &str) -> Result<AuthStatus, AuthError> {
    let email = email.trim();
    if email.len() > 320 || !email.contains('@') || password.is_empty() || password.len() > 1_024 {
        return Err(AuthError::InvalidCredentials);
    }
    let _guard = SESSIONS.session_lock.lock().await;
    let config = SupabaseConfig::load()?;
    let response = auth_client()?
        .post(format!("{}/auth/v1/token?grant_type=password", config.url))
        .header("apikey", &config.client_key)
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|error| AuthError::Remote(error.to_string()))?;
    if response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::UNAUTHORIZED
    {
        return Err(AuthError::InvalidCredentials);
    }
    if !response.status().is_success() {
        return Err(AuthError::Remote(format!("HTTP {}", response.status())));
    }
    let token: TokenResponse = response
        .json()
        .await
        .map_err(|error| AuthError::Remote(error.to_string()))?;
    let session = StoredSession {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: Utc::now() + Duration::seconds(token.expires_in.max(60)),
        user_id: token.user.id,
        email: token.user.email.unwrap_or_else(|| email.to_owned()),
    };
    SESSIONS.save(&session)?;
    Ok(status())
}

pub async fn valid_session() -> Result<StoredSession, AuthError> {
    SESSIONS.valid_session(SupabaseConfig::load).await
}

async fn refresh_session(
    config: &SupabaseConfig,
    refresh_token: &str,
) -> Result<StoredSession, AuthError> {
    let response = auth_client()?
        .post(format!(
            "{}/auth/v1/token?grant_type=refresh_token",
            config.url
        ))
        .header("apikey", &config.client_key)
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .map_err(|error| AuthError::Remote(error.to_string()))?;
    if matches!(
        response.status(),
        StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED
    ) {
        return Err(AuthError::SessionExpired);
    }
    if !response.status().is_success() {
        return Err(AuthError::Remote(format!(
            "Session refresh returned HTTP {}",
            response.status()
        )));
    }
    let token: TokenResponse = response
        .json()
        .await
        .map_err(|error| AuthError::Remote(error.to_string()))?;
    let session = StoredSession {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: Utc::now() + Duration::seconds(token.expires_in.max(60)),
        user_id: token.user.id,
        email: token.user.email.unwrap_or_default(),
    };
    Ok(session)
}

pub async fn sign_out() -> Result<AuthStatus, AuthError> {
    let _guard = SESSIONS.session_lock.lock().await;
    if let (Ok(config), Ok(session), Ok(client)) =
        (SupabaseConfig::load(), SESSIONS.store.load(), auth_client())
    {
        let _ = client
            .post(format!("{}/auth/v1/logout", config.url))
            .header("apikey", &config.client_key)
            .bearer_auth(&session.access_token)
            .send()
            .await;
    }
    SESSIONS.rejected.store(true, Ordering::SeqCst);
    SESSIONS.store.delete()?;
    Ok(status())
}

fn auth_client() -> Result<reqwest::Client, AuthError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| AuthError::Remote(error.to_string()))
}

fn entry() -> Result<Entry, AuthError> {
    Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|error| AuthError::Credential(error.to_string()))
}

// Production credentials have exactly one persistence boundary: the OS vault.
// Tests inject an isolated in-memory store and never touch a user's credentials.
trait SessionStore {
    fn load(&self) -> Result<StoredSession, AuthError>;
    fn save(&self, session: &StoredSession) -> Result<(), AuthError>;
    fn delete(&self) -> Result<(), AuthError>;
}

struct NativeSessionStore;

impl SessionStore for NativeSessionStore {
    fn load(&self) -> Result<StoredSession, AuthError> {
        let serialized = entry()?.get_password().map_err(|error| match error {
            keyring::Error::NoEntry => AuthError::SessionExpired,
            error => AuthError::Credential(error.to_string()),
        })?;
        serde_json::from_str(&serialized).map_err(|_| AuthError::InvalidSession)
    }

    fn save(&self, session: &StoredSession) -> Result<(), AuthError> {
        let serialized = serde_json::to_string(session).map_err(|_| AuthError::InvalidSession)?;
        entry()?
            .set_password(&serialized)
            .map_err(|error| AuthError::Credential(error.to_string()))
    }

    fn delete(&self) -> Result<(), AuthError> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AuthError::Credential(error.to_string())),
        }
    }
}

struct SessionManager<S> {
    store: S,
    // Serialize refresh, sign-in and sign-out so token rotation cannot race with
    // another refresh or resurrect a session after the user signs out.
    session_lock: Mutex<()>,
    // Keep a rejected session unusable even if the OS vault refuses deletion.
    // Only a successful sign-in resets this flag; it never contains credentials.
    rejected: AtomicBool,
}

impl<S: SessionStore> SessionManager<S> {
    const fn new(store: S) -> Self {
        Self {
            store,
            session_lock: Mutex::const_new(()),
            rejected: AtomicBool::new(false),
        }
    }

    fn status(&self, configured: bool) -> AuthStatus {
        let session = if configured && !self.rejected.load(Ordering::SeqCst) {
            self.store
                .load()
                .ok()
                .filter(|_| !self.rejected.load(Ordering::SeqCst))
        } else {
            None
        };
        AuthStatus {
            configured,
            signed_in: session.is_some(),
            email: session.as_ref().map(|value| value.email.clone()),
            expires_at: session.map(|value| value.expires_at),
        }
    }

    // Caller holds session_lock while replacing credentials.
    fn save(&self, session: &StoredSession) -> Result<(), AuthError> {
        self.store.save(session)?;
        self.rejected.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn valid_session(
        &self,
        config: impl FnOnce() -> Result<SupabaseConfig, AuthError>,
    ) -> Result<StoredSession, AuthError> {
        let _guard = self.session_lock.lock().await;
        if self.rejected.load(Ordering::SeqCst) {
            return Err(AuthError::SessionExpired);
        }
        let session = self.store.load()?;
        if session.expires_at > Utc::now() + Duration::seconds(60) {
            return Ok(session);
        }
        match refresh_session(&config()?, &session.refresh_token).await {
            Ok(session) => {
                self.save(&session)?;
                Ok(session)
            }
            Err(AuthError::SessionExpired) => {
                self.rejected.store(true, Ordering::SeqCst);
                self.store.delete()?;
                Err(AuthError::SessionExpired)
            }
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::AtomicUsize;
    use std::thread;
    use std::time::{Duration as StdDuration, Instant};

    struct MemoryStore {
        session: StdMutex<Option<StoredSession>>,
        deletes: AtomicUsize,
        saves: AtomicUsize,
        fail_delete: bool,
    }

    impl MemoryStore {
        fn new(session: StoredSession) -> Self {
            Self {
                session: StdMutex::new(Some(session)),
                deletes: AtomicUsize::new(0),
                saves: AtomicUsize::new(0),
                fail_delete: false,
            }
        }
    }

    impl SessionStore for MemoryStore {
        fn load(&self) -> Result<StoredSession, AuthError> {
            self.session
                .lock()
                .unwrap()
                .clone()
                .ok_or(AuthError::SessionExpired)
        }

        fn save(&self, session: &StoredSession) -> Result<(), AuthError> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            *self.session.lock().unwrap() = Some(session.clone());
            Ok(())
        }

        fn delete(&self) -> Result<(), AuthError> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            if self.fail_delete {
                return Err(AuthError::Credential("test vault unavailable".into()));
            }
            *self.session.lock().unwrap() = None;
            Ok(())
        }
    }

    fn session(seconds: i64) -> StoredSession {
        StoredSession {
            access_token: "test-access-token".into(),
            refresh_token: "test-refresh-token".into(),
            expires_at: Utc::now() + Duration::seconds(seconds),
            user_id: "test-user".into(),
            email: "test@example.com".into(),
        }
    }

    // One real HTTP exchange with synthetic credentials. No OS-vault or remote
    // Supabase access, environment mutation, or new HTTP-mocking dependency.
    fn refresh_server(response: Option<(u16, &str)>) -> (SupabaseConfig, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let config = SupabaseConfig {
            url: format!("http://{}", listener.local_addr().unwrap()),
            client_key: "test-publishable-key".into(),
        };
        let response = response.map(|(status, body)| (status, body.to_owned()));
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + StdDuration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "refresh request never arrived");
                        thread::sleep(StdDuration::from_millis(5));
                    }
                    Err(error) => panic!("test server accept failed: {error}"),
                }
            };
            // Accepted sockets can inherit the listener's nonblocking flag on
            // macOS. The bounded request reader below expects blocking reads.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(StdDuration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            loop {
                let mut bytes = [0; 1024];
                let read = stream.read(&mut bytes).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&bytes[..read]);
                if let Some(end) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let content_length: usize = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + content_length {
                        assert!(
                            headers.starts_with(
                                "POST /auth/v1/token?grant_type=refresh_token HTTP/1.1"
                            )
                        );
                        let body: serde_json::Value =
                            serde_json::from_slice(&request[end + 4..]).unwrap();
                        assert_eq!(body["refresh_token"], "test-refresh-token");
                        break;
                    }
                }
            }
            if let Some((status, body)) = response {
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            // With no response, dropping the connection simulates a network failure.
        });
        (config, handle)
    }

    const REFRESHED: &str = r#"{"access_token":"rotated-access-token","refresh_token":"rotated-refresh-token","expires_in":3600,"user":{"id":"test-user","email":"test@example.com"}}"#;

    #[tokio::test]
    async fn valid_unexpired_session_does_not_refresh_or_write_credentials() {
        let manager = SessionManager::new(MemoryStore::new(session(3600)));
        let current = manager
            .valid_session(|| panic!("unexpired sessions need no HTTP request"))
            .await
            .unwrap();
        assert_eq!(current.access_token, "test-access-token");
        assert_eq!(manager.store.saves.load(Ordering::SeqCst), 0);
        assert!(manager.status(true).signed_in);
        assert_eq!(
            manager.status(true).email.as_deref(),
            Some("test@example.com")
        );
        assert!(!manager.status(false).signed_in);
    }

    #[tokio::test]
    async fn refresh_rotates_and_persists_both_tokens() {
        let manager = SessionManager::new(MemoryStore::new(session(-1)));
        let (config, server) = refresh_server(Some((200, REFRESHED)));
        let refreshed = manager.valid_session(|| Ok(config)).await.unwrap();
        server.join().unwrap();
        assert_eq!(refreshed.access_token, "rotated-access-token");
        assert_eq!(refreshed.refresh_token, "rotated-refresh-token");
        assert!(refreshed.expires_at > Utc::now() + Duration::seconds(3500));
        let stored = manager.store.load().unwrap();
        assert_eq!(stored.access_token, refreshed.access_token);
        assert_eq!(stored.refresh_token, refreshed.refresh_token);
        assert_eq!(manager.store.saves.load(Ordering::SeqCst), 1);
        let next = manager
            .valid_session(|| panic!("rotated token must be reused"))
            .await
            .unwrap();
        assert_eq!(next.refresh_token, refreshed.refresh_token);
    }

    async fn assert_rejected(status: u16) {
        let manager = SessionManager::new(MemoryStore::new(session(-1)));
        assert!(manager.status(true).signed_in);
        let (config, server) = refresh_server(Some((
            status,
            r#"{"error_code":"refresh_token_not_found"}"#,
        )));
        assert!(matches!(
            manager.valid_session(|| Ok(config)).await,
            Err(AuthError::SessionExpired)
        ));
        server.join().unwrap();
        assert!(manager.store.session.lock().unwrap().is_none());
        let status = manager.status(true);
        assert!(!status.signed_in);
        assert!(status.email.is_none());
        assert!(status.expires_at.is_none());
        for _ in 0..3 {
            assert!(matches!(
                manager
                    .valid_session(|| panic!("rejected session must never retry"))
                    .await,
                Err(AuthError::SessionExpired)
            ));
        }
        assert_eq!(manager.store.deletes.load(Ordering::SeqCst), 1);
        // Reauthentication can replace the rejected credential and reconnect.
        manager.save(&session(3600)).unwrap();
        assert!(manager.status(true).signed_in);
        assert!(
            manager
                .valid_session(|| panic!("new session is valid"))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn refresh_http_400_clears_session_and_requires_reauthentication_once() {
        assert_rejected(400).await;
    }

    #[tokio::test]
    async fn refresh_http_401_clears_session_and_requires_reauthentication_once() {
        assert_rejected(401).await;
    }

    async fn assert_preserved(response: Option<(u16, &str)>) {
        let manager = SessionManager::new(MemoryStore::new(session(-1)));
        let original_expiry = manager.store.load().unwrap().expires_at;
        let (config, server) = refresh_server(response);
        assert!(matches!(
            manager.valid_session(|| Ok(config)).await,
            Err(AuthError::Remote(_))
        ));
        server.join().unwrap();
        let stored = manager.store.load().unwrap();
        assert_eq!(stored.access_token, "test-access-token");
        assert_eq!(stored.refresh_token, "test-refresh-token");
        assert_eq!(stored.expires_at, original_expiry);
        assert!(manager.status(true).signed_in);
        assert_eq!(manager.store.deletes.load(Ordering::SeqCst), 0);
        assert_eq!(manager.store.saves.load(Ordering::SeqCst), 0);
        // A later manual/background attempt may succeed after a transient failure.
        let (config, server) = refresh_server(Some((200, REFRESHED)));
        assert!(manager.valid_session(|| Ok(config)).await.is_ok());
        server.join().unwrap();
    }

    #[tokio::test]
    async fn refresh_5xx_preserves_credentials_and_can_recover() {
        for status in [500, 502, 503] {
            assert_preserved(Some((status, "{}"))).await;
        }
    }

    #[tokio::test]
    async fn refresh_network_failure_preserves_credentials_and_can_recover() {
        assert_preserved(None).await;
    }

    #[tokio::test]
    async fn refresh_rate_limit_and_malformed_success_preserve_credentials() {
        assert_preserved(Some((429, "{}"))).await;
        assert_preserved(Some((200, "{}"))).await;
    }

    #[tokio::test]
    async fn concurrent_callers_refresh_only_once_and_share_rotated_credentials() {
        let manager = SessionManager::new(MemoryStore::new(session(-1)));
        let (config, server) = refresh_server(Some((200, REFRESHED)));
        let (first, second) = tokio::join!(
            manager.valid_session(|| Ok(config.clone())),
            manager.valid_session(|| Ok(config.clone())),
        );
        server.join().unwrap();
        assert_eq!(first.unwrap().refresh_token, "rotated-refresh-token");
        assert_eq!(second.unwrap().refresh_token, "rotated-refresh-token");
        assert_eq!(manager.store.saves.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn vault_delete_failure_still_blocks_rejected_session_and_retries() {
        let store = MemoryStore {
            fail_delete: true,
            ..MemoryStore::new(session(-1))
        };
        let manager = SessionManager::new(store);
        let (config, server) = refresh_server(Some((400, "{}")));
        assert!(matches!(
            manager.valid_session(|| Ok(config)).await,
            Err(AuthError::Credential(_))
        ));
        server.join().unwrap();
        assert!(!manager.status(true).signed_in);
        assert!(matches!(
            manager
                .valid_session(|| panic!("must not retry rejected token"))
                .await,
            Err(AuthError::SessionExpired)
        ));
        assert_eq!(manager.store.deletes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn auth_status_serialization_never_exposes_tokens() {
        let manager = SessionManager::new(MemoryStore::new(session(3600)));
        let status = serde_json::to_value(manager.status(true)).unwrap();
        assert_eq!(status.as_object().unwrap().len(), 4);
        assert!(status.get("access_token").is_none());
        assert!(status.get("refresh_token").is_none());
    }
}
