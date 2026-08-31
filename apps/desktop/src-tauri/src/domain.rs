use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementKind {
    Measured,
    Estimated,
    ActivityOnly,
}

impl MeasurementKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Estimated => "estimated",
            Self::ActivityOnly => "activity_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    LocalCli,
    Browser,
    Api,
    Manual,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalCli => "local_cli",
            Self::Browser => "browser",
            Self::Api => "api",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCounts {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_write_5m_tokens: i64,
    pub cache_write_1h_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
}

impl TokenCounts {
    pub fn normalize(mut self) -> Self {
        self.input_tokens = self.input_tokens.max(0);
        self.cached_input_tokens = self.cached_input_tokens.max(0);
        self.cache_write_tokens = self.cache_write_tokens.max(0);
        self.cache_write_5m_tokens = self.cache_write_5m_tokens.max(0);
        self.cache_write_1h_tokens = self.cache_write_1h_tokens.max(0);
        self.cache_write_tokens = self.cache_write_tokens.max(
            self.cache_write_5m_tokens
                .saturating_add(self.cache_write_1h_tokens),
        );
        self.output_tokens = self.output_tokens.max(0);
        self.reasoning_tokens = self.reasoning_tokens.max(0);
        let derived = self.input_tokens.saturating_add(self.output_tokens);
        if self.total_tokens <= 0 {
            self.total_tokens = derived;
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEvent {
    pub id: String,
    pub provider: String,
    pub source: String,
    pub source_type: SourceType,
    pub native_session_id: String,
    pub native_event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub model: Option<String>,
    pub project_name: Option<String>,
    #[serde(flatten)]
    pub tokens: TokenCounts,
    pub estimated_api_value_usd_micros: Option<i64>,
    /// Provider-recorded cost in exact 1e-10 USD ticks. This is not an ArcMeter estimate.
    pub native_cost_usd_ticks: Option<i64>,
    pub pricing_status: String,
    pub measurement_kind: MeasurementKind,
    pub device_id: String,
    pub created_at: DateTime<Utc>,
}

impl UsageEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn measured(
        provider: impl Into<String>,
        source: impl Into<String>,
        native_session_id: impl Into<String>,
        native_event_id: impl Into<String>,
        occurred_at: DateTime<Utc>,
        model: Option<String>,
        project_name: Option<String>,
        tokens: TokenCounts,
        device_id: impl Into<String>,
    ) -> Self {
        let provider = provider.into();
        let native_session_id = native_session_id.into();
        let native_event_id = native_event_id.into();
        let id = deterministic_event_id(&provider, &native_session_id, &native_event_id);
        Self {
            id,
            provider,
            source: source.into(),
            source_type: SourceType::LocalCli,
            native_session_id,
            native_event_id,
            occurred_at,
            model: clean_label(model),
            project_name: clean_label(project_name).and_then(|value| sanitize_project_name(&value)),
            tokens: tokens.normalize(),
            estimated_api_value_usd_micros: None,
            native_cost_usd_ticks: None,
            pricing_status: "unavailable".into(),
            measurement_kind: MeasurementKind::Measured,
            device_id: device_id.into(),
            created_at: Utc::now(),
        }
    }

    pub fn with_native_cost_usd_ticks(mut self, ticks: Option<i64>) -> Self {
        self.native_cost_usd_ticks = ticks.filter(|value| *value >= 0);
        self
    }

    pub fn activity(
        provider: impl Into<String>,
        source: impl Into<String>,
        source_type: SourceType,
        minute_epoch: i64,
        device_id: impl Into<String>,
    ) -> Option<Self> {
        let provider = provider.into();
        let source = source.into();
        let device_id = device_id.into();
        let occurred_at = DateTime::from_timestamp(minute_epoch.checked_mul(60)?, 0)?;
        let native_session_id = format!("{source}:{device_id}");
        let native_event_id = format!("minute:{minute_epoch}");
        let id = deterministic_event_id(&provider, &native_session_id, &native_event_id);
        Some(Self {
            id,
            provider,
            source,
            source_type,
            native_session_id,
            native_event_id,
            occurred_at,
            model: None,
            project_name: None,
            tokens: TokenCounts::default(),
            estimated_api_value_usd_micros: None,
            native_cost_usd_ticks: None,
            pricing_status: "unavailable".into(),
            measurement_kind: MeasurementKind::ActivityOnly,
            device_id,
            created_at: Utc::now(),
        })
    }
}

pub fn deterministic_event_id(provider: &str, session_id: &str, native_event_id: &str) -> String {
    let canonical = format!("{provider}\u{1f}{session_id}\u{1f}{native_event_id}");
    let digest = Sha256::digest(canonical.as_bytes());
    hex::encode(digest)
}

pub fn fallback_event_fingerprint(parts: &[&str]) -> String {
    let canonical = parts.join("\u{1f}");
    format!(
        "fingerprint:{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    )
}

pub fn sanitize_project_name(raw: &str) -> Option<String> {
    let normalized = raw.trim().replace('\\', "/");
    let basename = normalized.rsplit('/').find(|segment| !segment.is_empty())?;
    let sanitized: String = basename
        .chars()
        .filter(|ch| !ch.is_control() && !matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        .take(96)
        .collect();
    let value = sanitized.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn clean_label(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let clean: String = item
            .trim()
            .chars()
            .filter(|ch| !ch.is_control())
            .take(128)
            .collect();
        (!clean.is_empty()).then_some(clean)
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorDiagnostic {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub record_number: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventReconciliation {
    pub legacy_event_id: String,
    pub replacement_event_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorOutput {
    pub events: Vec<UsageEvent>,
    pub reconciliation_hints: Vec<EventReconciliation>,
    pub diagnostics: Vec<CollectorDiagnostic>,
    pub records_seen: usize,
    pub records_ignored: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_id_is_stable_and_scoped() {
        let a = deterministic_event_id("codex", "session", "event");
        assert_eq!(a, deterministic_event_id("codex", "session", "event"));
        assert_ne!(a, deterministic_event_id("claude", "session", "event"));
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn project_sanitization_never_keeps_parent_paths() {
        assert_eq!(
            sanitize_project_name(r"C:\Users\Nazar\Client\ArcCard"),
            Some("ArcCard".into())
        );
        assert_eq!(
            sanitize_project_name("/Users/nazar/work/ArcMeter"),
            Some("ArcMeter".into())
        );
        assert_eq!(sanitize_project_name(""), None);
    }

    #[test]
    fn token_total_uses_provider_total_or_safe_derived_value() {
        let explicit = TokenCounts {
            input_tokens: 10,
            output_tokens: 2,
            total_tokens: 20,
            ..Default::default()
        }
        .normalize();
        assert_eq!(explicit.total_tokens, 20);
        let derived = TokenCounts {
            input_tokens: 10,
            output_tokens: 2,
            ..Default::default()
        }
        .normalize();
        assert_eq!(derived.total_tokens, 12);
    }

    #[test]
    fn activity_is_a_deterministic_zero_token_minute() {
        let first = UsageEvent::activity(
            "grok",
            "grok_web",
            SourceType::Browser,
            29_800_000,
            "device-1",
        )
        .unwrap();
        let second = UsageEvent::activity(
            "grok",
            "grok_web",
            SourceType::Browser,
            29_800_000,
            "device-1",
        )
        .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.tokens, TokenCounts::default());
        assert_eq!(first.measurement_kind, MeasurementKind::ActivityOnly);
        assert_eq!(first.source_type, SourceType::Browser);
    }
}
