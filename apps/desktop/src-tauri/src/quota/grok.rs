use super::{ExtraUsage, ProviderQuotaWindow, QuotaWindowKind};
use chrono::{DateTime, TimeDelta, Utc};
use reqwest::{Client, StatusCode, header::RETRY_AFTER};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use zeroize::Zeroize;

pub const BILLING_ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
pub const SETTINGS_ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/settings";
const XAI_ISSUER: &str = "https://auth.x.ai";
const TOKEN_HEADER: &str = "xai-grok-cli";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const LEGACY_TOKEN_TTL: TimeDelta = TimeDelta::days(30);

#[derive(Debug, Clone, PartialEq, Eq)]
struct CredentialEnvironment {
    grok_home: Option<PathBuf>,
    home_dir: Option<PathBuf>,
}

impl CredentialEnvironment {
    fn current() -> Self {
        Self {
            grok_home: std::env::var_os("GROK_HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            home_dir: directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned()),
        }
    }

    fn auth_path(&self) -> PathBuf {
        self.grok_home
            .clone()
            .or_else(|| self.home_dir.as_ref().map(|home| home.join(".grok")))
            .unwrap_or_else(|| PathBuf::from(".grok"))
            .join("auth.json")
    }
}

pub struct GrokCredential {
    access_token: String,
    user_id: String,
    pub expires_at: DateTime<Utc>,
}

impl Drop for GrokCredential {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.user_id.zeroize();
    }
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
    pub plan_label: Option<String>,
}

pub fn discover_credential() -> Result<GrokCredential, CredentialError> {
    read_credentials_file(&CredentialEnvironment::current().auth_path())
}

fn read_credentials_file(path: &Path) -> Result<GrokCredential, CredentialError> {
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
    let credential = parse_credential(&contents);
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

fn parse_credential(serialized: &str) -> Result<GrokCredential, CredentialError> {
    let store: BTreeMap<String, Value> =
        serde_json::from_str(serialized).map_err(|_| CredentialError::Invalid)?;
    let now = Utc::now();
    let selected = store
        .values()
        .filter_map(Value::as_object)
        .filter(|entry| is_supported_auth(entry))
        .filter_map(|entry| {
            let key = nonempty_string(entry, "key")?;
            let user_id = nonempty_string(entry, "user_id")?;
            let created_at = entry
                .get("create_time")
                .and_then(Value::as_str)
                .and_then(parse_datetime)?;
            let expires_at = entry
                .get("expires_at")
                .and_then(Value::as_str)
                .and_then(parse_datetime)
                .unwrap_or(created_at + LEGACY_TOKEN_TTL);
            Some((created_at, expires_at, key.to_owned(), user_id.to_owned()))
        })
        .max_by_key(|(created_at, _, _, _)| *created_at)
        .ok_or(CredentialError::Invalid)?;
    if selected.1 <= now {
        return Err(CredentialError::Expired);
    }
    Ok(GrokCredential {
        access_token: selected.2,
        user_id: selected.3,
        expires_at: selected.1,
    })
}

fn is_supported_auth(entry: &Map<String, Value>) -> bool {
    let mode = entry.get("auth_mode").and_then(Value::as_str);
    let issuer = entry.get("oidc_issuer").and_then(Value::as_str);
    matches!(mode, Some("oidc") | Some("external")) && issuer == Some(XAI_ISSUER)
}

fn nonempty_string<'a>(entry: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    entry
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub async fn fetch_usage(credential: &GrokCredential) -> Result<ParsedUsage, FetchFailure> {
    if credential.expires_at <= Utc::now() {
        return Err(FetchFailure::ExpiredLogin);
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| FetchFailure::Offline)?;
    let billing = send(&client, BILLING_ENDPOINT, credential).await?;
    let mut usage = parse_billing_response(&billing)?;
    if let Ok(settings) = send(&client, SETTINGS_ENDPOINT, credential).await {
        usage.plan_label = parse_plan_label(&settings);
        if let (Some(extra), Some(enabled)) = (
            usage.extra_usage.as_mut(),
            parse_on_demand_enabled(&settings),
        ) {
            extra.enabled = enabled;
        }
    }
    Ok(usage)
}

async fn send(
    client: &Client,
    endpoint: &str,
    credential: &GrokCredential,
) -> Result<Vec<u8>, FetchFailure> {
    let response = client
        .get(endpoint)
        .bearer_auth(&credential.access_token)
        .header("X-XAI-Token-Auth", TOKEN_HEADER)
        .header("x-userid", &credential.user_id)
        .header(
            "x-grok-client-version",
            concat!("arcmeter-", env!("CARGO_PKG_VERSION")),
        )
        .header("x-grok-client-mode", "headless")
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            concat!("arcmeter/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .map_err(|_| FetchFailure::Offline)?;
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
    let body = response
        .bytes()
        .await
        .map_err(|_| FetchFailure::InvalidResponse)?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(FetchFailure::InvalidResponse);
    }
    Ok(body.to_vec())
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

pub fn parse_billing_response(bytes: &[u8]) -> Result<ParsedUsage, FetchFailure> {
    let root: Value = serde_json::from_slice(bytes).map_err(|_| FetchFailure::InvalidResponse)?;
    let config = root
        .get("config")
        .and_then(Value::as_object)
        .ok_or(FetchFailure::InvalidResponse)?;
    let mut windows = Vec::new();
    if let Some(percent) = config.get("creditUsagePercent") {
        let utilization_bps = percent_to_bps(percent)?;
        let period = config
            .get("currentPeriod")
            .and_then(Value::as_object)
            .ok_or(FetchFailure::InvalidResponse)?;
        let period_type = nonempty_string(period, "type").ok_or(FetchFailure::InvalidResponse)?;
        let (key, label, kind) = match period_type {
            "USAGE_PERIOD_TYPE_WEEKLY" => ("weekly_pool", "Weekly", QuotaWindowKind::Weekly),
            "USAGE_PERIOD_TYPE_MONTHLY" => ("monthly_pool", "Monthly", QuotaWindowKind::Monthly),
            _ => ("credits_pool", "Credits", QuotaWindowKind::Other),
        };
        windows.push(ProviderQuotaWindow {
            key: key.into(),
            label: label.into(),
            kind,
            scope: sanitize_scope(period_type),
            utilization_bps,
            period_starts_at: parse_required_date(period, "start")?,
            resets_at: parse_required_date(period, "end")?,
        });
    } else if let Some(legacy) = parse_legacy_window(config)? {
        windows.push(legacy);
    }
    if let Some(products) = config.get("productUsage").and_then(Value::as_array) {
        for item in products {
            let Some(entry) = item.as_object() else {
                continue;
            };
            let Some(product) = nonempty_string(entry, "product") else {
                continue;
            };
            let Some(percent) = entry.get("usagePercent") else {
                continue;
            };
            let key = sanitize_key(product).ok_or(FetchFailure::InvalidResponse)?;
            windows.push(ProviderQuotaWindow {
                key: format!("product_{key}"),
                label: product_label(product),
                kind: QuotaWindowKind::Product,
                scope: sanitize_scope(product),
                utilization_bps: percent_to_bps(percent)?,
                period_starts_at: None,
                resets_at: None,
            });
        }
    }
    if windows.is_empty() {
        return Err(FetchFailure::InvalidResponse);
    }
    Ok(ParsedUsage {
        windows,
        extra_usage: parse_extra_usage(config),
        plan_label: root
            .get("subscriptionTier")
            .and_then(Value::as_str)
            .and_then(sanitize_label),
    })
}

fn parse_legacy_window(
    config: &Map<String, Value>,
) -> Result<Option<ProviderQuotaWindow>, FetchFailure> {
    let Some(limit) = cents(config.get("monthlyLimit")) else {
        return Ok(None);
    };
    let Some(used) = cents(config.get("used")) else {
        return Ok(None);
    };
    if limit == 0 || used > limit {
        return Err(FetchFailure::InvalidResponse);
    }
    let start = config
        .get("billingPeriodStart")
        .and_then(Value::as_str)
        .and_then(parse_datetime);
    let end = config
        .get("billingPeriodEnd")
        .and_then(Value::as_str)
        .and_then(parse_datetime);
    if config.get("billingPeriodStart").is_some() && start.is_none()
        || config.get("billingPeriodEnd").is_some() && end.is_none()
    {
        return Err(FetchFailure::InvalidResponse);
    }
    Ok(Some(ProviderQuotaWindow {
        key: "monthly_credits".into(),
        label: "Monthly".into(),
        kind: QuotaWindowKind::Monthly,
        scope: Some("legacy_monthly_credits".into()),
        utilization_bps: ((used as i128 * 10_000) / limit as i128) as i64,
        period_starts_at: start,
        resets_at: end,
    }))
}

fn parse_extra_usage(config: &Map<String, Value>) -> Option<ExtraUsage> {
    let cap = cents(config.get("onDemandCap"));
    let used = cents(config.get("onDemandUsed"));
    let prepaid = cents(config.get("prepaidBalance"));
    (cap.is_some() || used.is_some() || prepaid.is_some()).then(|| ExtraUsage {
        enabled: cap.unwrap_or_default() > 0 || used.unwrap_or_default() > 0,
        monthly_limit_minor: cap,
        used_credits_minor: used,
        prepaid_balance_minor: prepaid,
        utilization_bps: match (used, cap) {
            (Some(used), Some(cap)) if cap > 0 && used <= cap => {
                Some(((used as i128 * 10_000) / cap as i128) as i64)
            }
            _ => None,
        },
        currency: Some("USD".into()),
    })
}

fn cents(value: Option<&Value>) -> Option<i64> {
    let value = value?.get("val")?.as_i64()?;
    (value >= 0).then_some(value)
}

fn parse_required_date(
    entry: &Map<String, Value>,
    key: &str,
) -> Result<Option<DateTime<Utc>>, FetchFailure> {
    let Some(value) = entry.get(key) else {
        return Ok(None);
    };
    let parsed = value
        .as_str()
        .and_then(parse_datetime)
        .ok_or(FetchFailure::InvalidResponse)?;
    Ok(Some(parsed))
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

fn percent_to_bps(value: &Value) -> Result<i64, FetchFailure> {
    let number = value.as_f64().ok_or(FetchFailure::InvalidResponse)?;
    if !number.is_finite() || !(0.0..=100.0).contains(&number) {
        return Err(FetchFailure::InvalidResponse);
    }
    Ok((number * 100.0).round() as i64)
}

fn sanitize_key(value: &str) -> Option<String> {
    let output = value
        .chars()
        .take(80)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    (!output.is_empty()).then_some(output)
}

fn sanitize_scope(value: &str) -> Option<String> {
    let output = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(80)
        .collect::<String>();
    (!output.is_empty()).then_some(output)
}

fn sanitize_label(value: &str) -> Option<String> {
    let output = value
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect::<String>();
    (!output.trim().is_empty()).then_some(output)
}

fn product_label(product: &str) -> String {
    product
        .strip_prefix("PRODUCT_")
        .unwrap_or(product)
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            if part.eq_ignore_ascii_case("grok") {
                "Grok".into()
            } else if part.eq_ignore_ascii_case("api") {
                "API".into()
            } else {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|first| {
                        first.to_ascii_uppercase().to_string()
                            + &chars.as_str().to_ascii_lowercase()
                    })
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn parse_plan_label(bytes: &[u8]) -> Option<String> {
    let root: Value = serde_json::from_slice(bytes).ok()?;
    root.get("subscription_tier_display")
        .or_else(|| root.get("subscriptionTierDisplay"))
        .or_else(|| {
            root.get("settings")
                .and_then(|settings| settings.get("subscription_tier_display"))
        })
        .and_then(Value::as_str)
        .and_then(sanitize_label)
}

fn parse_on_demand_enabled(bytes: &[u8]) -> Option<bool> {
    let root: Value = serde_json::from_slice(bytes).ok()?;
    root.get("on_demand_enabled")
        .or_else(|| root.get("onDemandEnabled"))
        .or_else(|| {
            root.get("settings")
                .and_then(|settings| settings.get("on_demand_enabled"))
        })
        .and_then(Value::as_bool)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(grok_home: Option<&str>) -> CredentialEnvironment {
        CredentialEnvironment {
            grok_home: grok_home.map(PathBuf::from),
            home_dir: Some(PathBuf::from("/test-home")),
        }
    }

    #[test]
    fn credential_path_uses_default_and_grok_home_without_recursion() {
        assert_eq!(
            environment(None).auth_path(),
            PathBuf::from("/test-home/.grok/auth.json")
        );
        assert_eq!(
            environment(Some("/custom/grok")).auth_path(),
            PathBuf::from("/custom/grok/auth.json")
        );
    }

    #[test]
    fn credential_parser_reads_verified_key_and_rejects_legacy_or_expired_auth() {
        let future = (Utc::now() + TimeDelta::hours(1)).to_rfc3339();
        let payload = format!(
            r#"{{"scope":{{"key":"secret-value","auth_mode":"oidc","create_time":"2026-09-01T00:00:00Z","user_id":"safe-user","expires_at":"{future}","oidc_issuer":"https://auth.x.ai"}}}}"#
        );
        let credential = parse_credential(&payload).unwrap();
        assert_eq!(credential.user_id, "safe-user");
        assert!(matches!(
            parse_credential(r#"{"scope":{"auth_mode":"oidc"}}"#),
            Err(CredentialError::Invalid)
        ));
        assert!(matches!(
            parse_credential(
                r#"{"scope":{"key":"never-report-me","auth_mode":"web_login","create_time":"2026-09-01T00:00:00Z","user_id":"u"}}"#
            ),
            Err(CredentialError::Invalid)
        ));
        drop(credential);
    }

    #[test]
    fn weekly_fixture_preserves_zero_percent_period_products_and_cents() {
        let parsed = parse_billing_response(include_bytes!(
            "../../tests/fixtures/grok_quota_current.json"
        ))
        .unwrap();
        assert_eq!(parsed.windows[0].utilization_bps, 0);
        assert_eq!(parsed.windows[0].kind, QuotaWindowKind::Weekly);
        assert!(parsed.windows[0].period_starts_at.is_some());
        assert_eq!(parsed.windows[1].label, "Chat");
        assert_eq!(parsed.windows[2].label, "Grok Build");
        assert_eq!(parsed.windows[3].label, "Future Thing");
        assert!(
            parsed.windows[1..]
                .iter()
                .all(|window| window.kind == QuotaWindowKind::Product)
        );
        let extra = parsed.extra_usage.unwrap();
        assert_eq!(extra.monthly_limit_minor, Some(5_000));
        assert_eq!(extra.used_credits_minor, Some(300));
        assert_eq!(extra.prepaid_balance_minor, Some(938));
        assert_eq!(extra.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn legacy_fixture_is_monthly_and_uses_verified_cent_values() {
        let parsed = parse_billing_response(include_bytes!(
            "../../tests/fixtures/grok_quota_legacy.json"
        ))
        .unwrap();
        assert_eq!(parsed.windows.len(), 1);
        assert_eq!(parsed.windows[0].kind, QuotaWindowKind::Monthly);
        assert_eq!(parsed.windows[0].utilization_bps, 2_500);
    }

    #[test]
    fn malformed_and_impossible_responses_are_rejected() {
        for payload in [
            br#"not json"#.as_slice(),
            br#"{"config":null}"#,
            br#"{"config":{"creditUsagePercent":10}}"#,
            br#"{"config":{"creditUsagePercent":101,"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY"}}}"#,
            br#"{"config":{"creditUsagePercent":10,"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","end":"bad"}}}"#,
        ] {
            assert_eq!(
                parse_billing_response(payload),
                Err(FetchFailure::InvalidResponse)
            );
        }
    }

    #[test]
    fn unknown_fields_and_missing_products_do_not_hide_valid_quota() {
        let parsed = parse_billing_response(
            br#"{"config":{"creditUsagePercent":42.5,"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-08-28T00:00:00Z","end":"2026-09-04T00:00:00Z"},"futureField":{"nested":true}}}"#,
        )
        .unwrap();
        assert_eq!(parsed.windows.len(), 1);
        assert_eq!(parsed.windows[0].utilization_bps, 4_250);
    }

    #[test]
    fn settings_parser_reads_only_the_plan_label() {
        assert_eq!(
            parse_plan_label(
                br#"{"subscription_tier_display":"SuperGrok Heavy","email":"private@example.com"}"#
            )
            .as_deref(),
            Some("SuperGrok Heavy")
        );
        assert_eq!(
            parse_on_demand_enabled(br#"{"on_demand_enabled":true}"#),
            Some(true)
        );
    }
}
