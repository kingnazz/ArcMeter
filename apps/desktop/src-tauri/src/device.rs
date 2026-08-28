use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub friendly_name: String,
    pub os: String,
    pub architecture: String,
    pub app_version: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub sync_status: String,
}

impl Device {
    pub fn new(app_version: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            friendly_name: default_friendly_name(),
            os: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            app_version: app_version.to_owned(),
            created_at: now,
            last_seen_at: now,
            last_sync_at: None,
            sync_status: "local_only".into(),
        }
    }
}

fn default_friendly_name() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .and_then(|name| clean_name(&name));
    host.unwrap_or_else(|| match std::env::consts::OS {
        "windows" => "Windows PC".into(),
        "macos" => "Mac".into(),
        _ => "ArcMeter device".into(),
    })
}

pub fn clean_name(raw: &str) -> Option<String> {
    let value: String = raw
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect();
    (!value.is_empty()).then_some(value)
}
