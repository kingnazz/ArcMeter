use chrono::{DateTime, Duration, Utc};
use keyring::Entry;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const KEYRING_SERVICE: &str = "com.arctechone.arcmeter";
const KEYRING_USER: &str = "supabase-session";

#[derive(Debug, Error)]
pub enum AuthError {
    #[error(
        "Supabase is not configured. Add the client-safe project URL and publishable key before signing in."
    )]
    NotConfigured,
    #[error("Invalid email or password")]
    InvalidCredentials,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Deserialize)]
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
    let configured = SupabaseConfig::is_configured();
    let session = load_session().ok();
    AuthStatus {
        configured,
        signed_in: configured && session.is_some(),
        email: session.as_ref().map(|value| value.email.clone()),
        expires_at: session.map(|value| value.expires_at),
    }
}

pub async fn sign_in(email: &str, password: &str) -> Result<AuthStatus, AuthError> {
    let email = email.trim();
    if email.len() > 320 || !email.contains('@') || password.is_empty() || password.len() > 1_024 {
        return Err(AuthError::InvalidCredentials);
    }
    let config = SupabaseConfig::load()?;
    let response = reqwest::Client::new()
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
    save_session(&session)?;
    Ok(status())
}

pub async fn valid_session() -> Result<StoredSession, AuthError> {
    let session = load_session()?;
    if session.expires_at > Utc::now() + Duration::seconds(60) {
        return Ok(session);
    }
    refresh_session(&session.refresh_token).await
}

async fn refresh_session(refresh_token: &str) -> Result<StoredSession, AuthError> {
    let config = SupabaseConfig::load()?;
    let response = reqwest::Client::new()
        .post(format!(
            "{}/auth/v1/token?grant_type=refresh_token",
            config.url
        ))
        .header("apikey", &config.client_key)
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .map_err(|error| AuthError::Remote(error.to_string()))?;
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
    save_session(&session)?;
    Ok(session)
}

pub async fn sign_out() -> Result<AuthStatus, AuthError> {
    if let (Ok(config), Ok(session)) = (SupabaseConfig::load(), load_session()) {
        let _ = reqwest::Client::new()
            .post(format!("{}/auth/v1/logout", config.url))
            .header("apikey", &config.client_key)
            .bearer_auth(&session.access_token)
            .send()
            .await;
    }
    delete_session()?;
    Ok(status())
}

fn entry() -> Result<Entry, AuthError> {
    Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|error| AuthError::Credential(error.to_string()))
}

fn load_session() -> Result<StoredSession, AuthError> {
    let serialized = entry()?
        .get_password()
        .map_err(|error| AuthError::Credential(error.to_string()))?;
    serde_json::from_str(&serialized).map_err(|_| AuthError::InvalidSession)
}

fn save_session(session: &StoredSession) -> Result<(), AuthError> {
    let serialized = serde_json::to_string(session).map_err(|_| AuthError::InvalidSession)?;
    entry()?
        .set_password(&serialized)
        .map_err(|error| AuthError::Credential(error.to_string()))
}

fn delete_session() -> Result<(), AuthError> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AuthError::Credential(error.to_string())),
    }
}
