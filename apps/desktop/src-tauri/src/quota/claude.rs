use super::{ExtraUsage, ProviderQuotaWindow, QuotaWindowKind};
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, header::RETRY_AFTER};
use serde_json::{Map, Value};
#[cfg(target_os = "macos")]
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zeroize::Zeroize;

pub const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
pub const OAUTH_BETA: &str = "oauth-2025-04-20";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

pub struct ClaudeCredential {
    access_token: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub source: CredentialSource,
}

impl Drop for ClaudeCredential {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    CredentialsFile,
    #[cfg(target_os = "macos")]
    MacOsKeychain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    Unavailable,
    PermissionDenied,
    Invalid,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchFailure {
    ExpiredLogin,
    Forbidden,
    RateLimited(Option<u64>),
    ProviderUnavailable,
    Offline,
    InvalidResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUsage {
    pub windows: Vec<ProviderQuotaWindow>,
    pub extra_usage: Option<ExtraUsage>,
}

pub fn discover_credential() -> Result<ClaudeCredential, CredentialError> {
    #[cfg(target_os = "macos")]
    {
        match read_macos_keychain() {
            Ok(credential) => return Ok(credential),
            Err(CredentialError::Unavailable) => {}
            Err(error) => return Err(error),
        }
    }
    read_credentials_file(&credentials_path())
}

fn credentials_path() -> PathBuf {
    if let Some(config_dir) =
        std::env::var_os("CLAUDE_CONFIG_DIR").filter(|value| !value.is_empty())
    {
        return PathBuf::from(config_dir).join(".credentials.json");
    }
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".claude").join(".credentials.json"))
        .unwrap_or_else(|| PathBuf::from(".claude").join(".credentials.json"))
}

fn read_credentials_file(path: &Path) -> Result<ClaudeCredential, CredentialError> {
    let metadata = std::fs::metadata(path).map_err(map_io_error)?;
    if !metadata.is_file() {
        return Err(CredentialError::Unavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(CredentialError::PermissionDenied);
        }
    }
    let mut contents = std::fs::read_to_string(path).map_err(map_io_error)?;
    let credential = parse_credential(&contents, CredentialSource::CredentialsFile);
    contents.zeroize();
    credential
}

fn map_io_error(error: std::io::Error) -> CredentialError {
    match error.kind() {
        std::io::ErrorKind::NotFound => CredentialError::Unavailable,
        std::io::ErrorKind::PermissionDenied => CredentialError::PermissionDenied,
        _ => CredentialError::Unavailable,
    }
}

#[cfg(target_os = "macos")]
fn read_macos_keychain() -> Result<ClaudeCredential, CredentialError> {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .map_err(|_| CredentialError::Unavailable)?;
    let service = macos_keychain_service();
    let entry = keyring::Entry::new(&service, &username).map_err(map_keyring_error)?;
    let mut payload = entry.get_password().map_err(map_keyring_error)?;
    let credential = parse_credential(&payload, CredentialSource::MacOsKeychain);
    payload.zeroize();
    credential
}

#[cfg(target_os = "macos")]
fn macos_keychain_service() -> String {
    let Some(config_dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|value| !value.is_empty())
    else {
        return "Claude Code-credentials".into();
    };
    let path = PathBuf::from(config_dir);
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|current| current.join(&path))
            .unwrap_or(path)
    };
    let resolved = std::fs::canonicalize(&absolute).unwrap_or(absolute);
    let digest = hex::encode(Sha256::digest(resolved.to_string_lossy().as_bytes()));
    format!("Claude Code-credentials-{}", &digest[..8])
}

#[cfg(target_os = "macos")]
fn map_keyring_error(error: keyring::Error) -> CredentialError {
    match error {
        keyring::Error::NoEntry => CredentialError::Unavailable,
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            CredentialError::PermissionDenied
        }
        _ => CredentialError::Invalid,
    }
}

fn parse_credential(
    serialized: &str,
    source: CredentialSource,
) -> Result<ClaudeCredential, CredentialError> {
    let root: Value = serde_json::from_str(serialized).map_err(|_| CredentialError::Invalid)?;
    let oauth = root
        .get("claudeAiOauth")
        .and_then(Value::as_object)
        .ok_or(CredentialError::Invalid)?;
    let access_token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(CredentialError::Invalid)?
        .to_owned();
    let expires_at = oauth.get("expiresAt").and_then(parse_expiration);
    if expires_at.is_some_and(|value| value <= Utc::now()) {
        return Err(CredentialError::Expired);
    }
    Ok(ClaudeCredential {
        access_token,
        expires_at,
        source,
    })
}

fn parse_expiration(value: &Value) -> Option<DateTime<Utc>> {
    let milliseconds = value
        .as_i64()
        .or_else(|| value.as_str().and_then(|item| item.parse::<i64>().ok()))?;
    DateTime::from_timestamp_millis(milliseconds)
}

pub async fn fetch_usage(credential: &ClaudeCredential) -> Result<ParsedUsage, FetchFailure> {
    if credential
        .expires_at
        .is_some_and(|value| value <= Utc::now())
    {
        return Err(FetchFailure::ExpiredLogin);
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| FetchFailure::Offline)?;
    let response = client
        .get(USAGE_ENDPOINT)
        .bearer_auth(&credential.access_token)
        .header("anthropic-beta", OAUTH_BETA)
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            concat!("arcmeter/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .map_err(|error| {
            let _ = error;
            FetchFailure::Offline
        })?;
    match response.status() {
        StatusCode::UNAUTHORIZED => return Err(FetchFailure::ExpiredLogin),
        StatusCode::FORBIDDEN => return Err(FetchFailure::Forbidden),
        StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_retry_after);
            return Err(FetchFailure::RateLimited(retry_after));
        }
        status if status.is_server_error() => return Err(FetchFailure::ProviderUnavailable),
        status if !status.is_success() => return Err(FetchFailure::InvalidResponse),
        _ => {}
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(FetchFailure::InvalidResponse);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| FetchFailure::InvalidResponse)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(FetchFailure::InvalidResponse);
    }
    parse_usage_response(&bytes)
}

fn parse_retry_after(value: &str) -> Option<u64> {
    if let Some(seconds) = value.parse::<u64>().ok().filter(|seconds| *seconds > 0) {
        return Some(seconds);
    }
    let retry_at = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let seconds = (retry_at - Utc::now()).num_seconds();
    (seconds > 0).then_some(seconds as u64)
}

pub fn parse_usage_response(bytes: &[u8]) -> Result<ParsedUsage, FetchFailure> {
    let root: Value = serde_json::from_slice(bytes).map_err(|_| FetchFailure::InvalidResponse)?;
    let object = root.as_object().ok_or(FetchFailure::InvalidResponse)?;
    let mut windows = Vec::new();
    for (key, value) in object {
        if key == "extra_usage" || is_ignored_flat_key(key) {
            continue;
        }
        if let Some(window) = parse_flat_window(key, value) {
            windows.push(window);
        }
    }
    for container_key in ["limits", "windows", "quota_windows", "scoped_limits"] {
        if let Some(container) = object.get(container_key) {
            parse_dynamic_container(container, &mut windows);
        }
    }
    windows.sort_by(|left, right| {
        window_rank(left)
            .cmp(&window_rank(right))
            .then(left.key.cmp(&right.key))
    });
    windows.dedup_by(|left, right| left.key == right.key);
    if windows.is_empty() {
        return Err(FetchFailure::InvalidResponse);
    }
    Ok(ParsedUsage {
        windows,
        extra_usage: object.get("extra_usage").and_then(parse_extra_usage),
    })
}

fn parse_flat_window(key: &str, value: &Value) -> Option<ProviderQuotaWindow> {
    let entry = value.as_object()?;
    let (label, kind, scope) = match key {
        "five_hour" => ("5-hour".to_owned(), QuotaWindowKind::Rolling, None),
        "seven_day" => ("Weekly".to_owned(), QuotaWindowKind::Weekly, None),
        value if value.starts_with("seven_day_") => {
            let scope = value.trim_start_matches("seven_day_");
            if scope.is_empty() || is_unrelated_scope(scope) {
                return None;
            }
            (
                humanize(scope),
                QuotaWindowKind::ModelWeekly,
                Some(scope.to_owned()),
            )
        }
        _ => return None,
    };
    parse_window_fields(key, label, kind, scope, entry)
}

fn parse_dynamic_container(value: &Value, output: &mut Vec<ProviderQuotaWindow>) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                if let Some(window) = parse_dynamic_window(None, index, item) {
                    output.push(window);
                }
            }
        }
        Value::Object(items) => {
            for (index, (key, item)) in items.iter().enumerate() {
                if let Some(window) = parse_dynamic_window(Some(key), index, item) {
                    output.push(window);
                }
            }
        }
        _ => {}
    }
}

fn parse_dynamic_window(
    fallback_key: Option<&String>,
    index: usize,
    value: &Value,
) -> Option<ProviderQuotaWindow> {
    let entry = value.as_object()?;
    let scope = string_field(entry, &["scope", "model", "model_scope"]);
    let raw_kind = string_field(entry, &["kind", "type", "period"]);
    let key = string_field(entry, &["key", "id", "name"])
        .or_else(|| fallback_key.cloned())
        .or_else(|| scope.as_ref().map(|item| format!("weekly_{item}")))
        .unwrap_or_else(|| format!("dynamic_{index}"));
    if key == "seven_day_oauth_apps" || scope.as_deref().is_some_and(is_unrelated_scope) {
        return None;
    }
    let kind = match raw_kind.as_deref() {
        Some("rolling") | Some("five_hour") => QuotaWindowKind::Rolling,
        Some("weekly") | Some("seven_day") if scope.is_none() => QuotaWindowKind::Weekly,
        Some("model_weekly") | Some("model") | Some("scoped_weekly") => {
            QuotaWindowKind::ModelWeekly
        }
        _ if scope.is_some() => QuotaWindowKind::ModelWeekly,
        _ => QuotaWindowKind::Other,
    };
    let label = scope
        .as_ref()
        .map(|item| humanize(item))
        .unwrap_or_else(|| humanize(&key));
    parse_window_fields(&key, label, kind, scope, entry)
}

fn parse_window_fields(
    key: &str,
    label: String,
    kind: QuotaWindowKind,
    scope: Option<String>,
    entry: &Map<String, Value>,
) -> Option<ProviderQuotaWindow> {
    let utilization_bps = entry.get("utilization").and_then(percent_to_bps)?;
    let resets_at = entry
        .get("resets_at")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    Some(ProviderQuotaWindow {
        key: sanitize_key(key)?,
        label: sanitize_label(&label),
        kind,
        scope: scope.and_then(|value| sanitize_scope(&value)),
        utilization_bps,
        resets_at,
    })
}

fn parse_extra_usage(value: &Value) -> Option<ExtraUsage> {
    let entry = value.as_object()?;
    let enabled = entry.get("is_enabled")?.as_bool()?;
    let currency = entry
        .get("currency")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| {
            (3..=8).contains(&value.len()) && value.chars().all(|item| item.is_ascii_alphabetic())
        })
        .map(|value| value.to_ascii_uppercase());
    Some(ExtraUsage {
        enabled,
        monthly_limit_minor: nonnegative_integer(entry.get("monthly_limit")),
        used_credits_minor: nonnegative_integer(entry.get("used_credits")),
        utilization_bps: entry.get("utilization").and_then(percent_to_bps),
        currency,
    })
}

fn nonnegative_integer(value: Option<&Value>) -> Option<i64> {
    let number = value?.as_f64()?;
    if !number.is_finite() || number < 0.0 || number > i64::MAX as f64 {
        return None;
    }
    Some(number.round() as i64)
}

fn percent_to_bps(value: &Value) -> Option<i64> {
    let number = value.as_f64()?;
    if !number.is_finite() || !(0.0..=100.0).contains(&number) {
        return None;
    }
    Some((number * 100.0).round() as i64)
}

fn string_field(entry: &Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        entry
            .get(*name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn is_ignored_flat_key(key: &str) -> bool {
    matches!(
        key,
        "seven_day_oauth_apps"
            | "seven_day_cowork"
            | "seven_day_omelette"
            | "iguana_necktie"
            | "omelette_promotional"
    )
}

fn is_unrelated_scope(scope: &str) -> bool {
    matches!(
        scope,
        "oauth_apps" | "cowork" | "omelette" | "iguana_necktie"
    )
}

fn sanitize_key(value: &str) -> Option<String> {
    let output = value
        .chars()
        .take(80)
        .map(|item| {
            if item.is_ascii_alphanumeric() || matches!(item, '_' | '-') {
                item
            } else {
                '_'
            }
        })
        .collect::<String>();
    (!output.is_empty()).then_some(output)
}

fn sanitize_label(value: &str) -> String {
    let output = value
        .chars()
        .filter(|item| !item.is_control())
        .take(80)
        .collect::<String>();
    if output.trim().is_empty() {
        "Limit".into()
    } else {
        output
    }
}

fn sanitize_scope(value: &str) -> Option<String> {
    let output = value
        .chars()
        .filter(|item| item.is_ascii_alphanumeric() || matches!(item, '_' | '-'))
        .take(80)
        .collect::<String>();
    (!output.is_empty()).then_some(output)
}

fn humanize(value: &str) -> String {
    value
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn window_rank(window: &ProviderQuotaWindow) -> u8 {
    match window.kind {
        QuotaWindowKind::Rolling => 0,
        QuotaWindowKind::Weekly => 1,
        QuotaWindowKind::ModelWeekly => 2,
        QuotaWindowKind::Other => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn future_expiration() -> i64 {
        (Utc::now() + chrono::Duration::hours(1)).timestamp_millis()
    }

    #[test]
    fn parses_valid_credentials_without_exposing_secret_in_errors() {
        let secret = "synthetic-private-access-value";
        let json = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{secret}","expiresAt":{}}}}}"#,
            future_expiration()
        );
        let credential = parse_credential(&json, CredentialSource::CredentialsFile).unwrap();
        assert_eq!(credential.access_token, secret);
        for invalid in ["{}", r#"{"claudeAiOauth":{}}"#, "not-json"] {
            let error = match parse_credential(invalid, CredentialSource::CredentialsFile) {
                Err(error) => error,
                Ok(_) => panic!("invalid credential fixture unexpectedly parsed"),
            };
            assert!(!format!("{error:?}").contains(secret));
        }
    }

    #[test]
    fn rejects_expired_credentials_and_absent_files_safely() {
        let json = r#"{"claudeAiOauth":{"accessToken":"secret","expiresAt":1}}"#;
        assert!(matches!(
            parse_credential(json, CredentialSource::CredentialsFile),
            Err(CredentialError::Expired)
        ));
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            read_credentials_file(&directory.path().join("absent.json")),
            Err(CredentialError::Unavailable)
        ));
    }

    #[test]
    fn parses_flat_windows_decimals_nulls_models_extra_usage_and_ignores_oauth_apps() {
        let usage = parse_usage_response(include_bytes!(
            "../../tests/fixtures/claude_quota_flat.json"
        ))
        .unwrap();
        assert_eq!(usage.windows.len(), 3);
        assert_eq!(usage.windows[0].key, "five_hour");
        assert_eq!(usage.windows[0].utilization_bps, 4_763);
        assert_eq!(usage.windows[1].key, "seven_day");
        assert_eq!(usage.windows[2].scope.as_deref(), Some("opus"));
        assert!(
            !usage
                .windows
                .iter()
                .any(|item| item.key.contains("oauth_apps"))
        );
        let extra = usage.extra_usage.unwrap();
        assert!(extra.enabled);
        assert_eq!(extra.monthly_limit_minor, Some(5_000));
        assert_eq!(extra.used_credits_minor, Some(1_242));
        assert_eq!(extra.utilization_bps, Some(2_484));
        assert_eq!(extra.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn parses_dynamic_model_windows_and_missing_reset() {
        let usage = parse_usage_response(include_bytes!(
            "../../tests/fixtures/claude_quota_dynamic.json"
        ))
        .unwrap();
        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[0].label, "Fable");
        assert_eq!(usage.windows[0].utilization_bps, 813);
        assert_eq!(usage.windows[1].scope.as_deref(), Some("sonnet"));
        assert!(usage.windows[1].resets_at.is_none());
    }

    #[test]
    fn supports_future_flat_model_key_and_disabled_extra_usage() {
        let usage = parse_usage_response(br#"{
          "five_hour":{"utilization":0,"resets_at":null},
          "seven_day_fable":{"utilization":12.5,"resets_at":null},
          "extra_usage":{"is_enabled":false,"monthly_limit":null,"used_credits":null,"utilization":null,"currency":null}
        }"#).unwrap();
        assert_eq!(usage.windows[1].scope.as_deref(), Some("fable"));
        assert!(!usage.extra_usage.unwrap().enabled);
    }

    #[test]
    fn supports_object_shaped_scoped_windows() {
        let usage = parse_usage_response(
            br#"{
              "five_hour":{"utilization":2,"resets_at":null},
              "scoped_limits":{
                "weekly_future_model":{"kind":"model_weekly","scope":"future-model","utilization":6.75,"resets_at":"2026-09-07T00:00:00Z"}
              }
            }"#,
        )
        .unwrap();
        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[1].key, "weekly_future_model");
        assert_eq!(usage.windows[1].scope.as_deref(), Some("future-model"));
        assert_eq!(usage.windows[1].utilization_bps, 675);
    }

    #[test]
    fn invalid_utilization_and_malformed_payload_are_rejected() {
        assert!(matches!(
            parse_usage_response(br#"{"five_hour":{"utilization":101}}"#),
            Err(FetchFailure::InvalidResponse)
        ));
        assert!(matches!(
            parse_usage_response(br#"{"five_hour":{"utilization":"secret"}}"#),
            Err(FetchFailure::InvalidResponse)
        ));
        assert!(matches!(
            parse_usage_response(b"not-json"),
            Err(FetchFailure::InvalidResponse)
        ));
    }

    #[test]
    fn retry_after_requires_a_positive_delay() {
        assert_eq!(parse_retry_after("900"), Some(900));
        assert_eq!(parse_retry_after("0"), None);
        assert_eq!(parse_retry_after("not-a-delay"), None);
    }
}
