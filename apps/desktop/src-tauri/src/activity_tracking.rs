use crate::db::Database;
use crate::domain::{SourceType, UsageEvent};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;
use uuid::Uuid;

pub const BRIDGE_PORT: u16 = 47_639;
const MAX_REQUEST_BYTES: usize = 8_192;
const MAX_MINUTE_DRIFT: i64 = 5;
const BRIDGE_TOKEN_KEY: &str = "activity_bridge_token";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityTrackingStatus {
    pub claude_desktop_supported: bool,
    pub claude_desktop_enabled: bool,
    pub claude_desktop_minutes: i64,
    pub claude_desktop_last_activity_at: Option<DateTime<Utc>>,
    pub browser_bridge_enabled: bool,
    pub browser_bridge_port: u16,
    pub pairing_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrowserActivityPayload {
    source: String,
    minute_epoch: i64,
}

pub fn ensure_bridge_token(database: &Database) -> crate::db::Result<String> {
    database.ensure_private_setting(BRIDGE_TOKEN_KEY, || {
        format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
    })
}

pub fn status(database: &Database) -> crate::db::Result<ActivityTrackingStatus> {
    let connection = rusqlite::Connection::open(database.path())?;
    let (claude_desktop_minutes, last_activity_at) = connection.query_row(
        "SELECT COUNT(*), MAX(occurred_at) FROM usage_events
         WHERE source = 'claude_desktop' AND measurement_kind = 'activity_only'",
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
    )?;
    let claude_desktop_last_activity_at = last_activity_at
        .map(|value| {
            DateTime::parse_from_rfc3339(&value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|error| crate::db::DatabaseError::Invalid(error.to_string()))
        })
        .transpose()?;
    Ok(ActivityTrackingStatus {
        claude_desktop_supported: cfg!(target_os = "macos"),
        claude_desktop_enabled: setting_enabled(database, "activity_claude_desktop_enabled"),
        claude_desktop_minutes,
        claude_desktop_last_activity_at,
        browser_bridge_enabled: setting_enabled(database, "activity_browser_bridge_enabled"),
        browser_bridge_port: BRIDGE_PORT,
        pairing_token: ensure_bridge_token(database)?,
    })
}

pub fn start_browser_bridge(database: Database, device_id: String) {
    std::thread::Builder::new()
        .name("arcmeter-browser-activity".into())
        .spawn(move || {
            let Ok(listener) = TcpListener::bind(("127.0.0.1", BRIDGE_PORT)) else {
                return;
            };
            for stream in listener.incoming().flatten() {
                handle_connection(stream, &database, &device_id);
            }
        })
        .ok();
}

fn handle_connection(mut stream: TcpStream, database: &Database, device_id: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut bytes = Vec::with_capacity(1_024);
    let mut chunk = [0_u8; 1_024];
    while bytes.len() < MAX_REQUEST_BYTES {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                bytes.extend_from_slice(&chunk[..read]);
                if request_is_complete(&bytes) {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let response = process_request(&bytes, database, device_id);
    let _ = stream.write_all(response.as_bytes());
}

fn process_request(bytes: &[u8], database: &Database, device_id: &str) -> String {
    let Ok(request) = std::str::from_utf8(bytes) else {
        return http_response(400, "invalid request");
    };
    let Some((headers, body)) = request.split_once("\r\n\r\n") else {
        return http_response(400, "invalid request");
    };
    let mut lines = headers.lines();
    let request_line = lines.next().unwrap_or_default();
    if request_line == "OPTIONS /v1/activity HTTP/1.1" {
        return http_response(204, "");
    }
    if request_line != "POST /v1/activity HTTP/1.1" {
        return http_response(404, "not found");
    }
    if !setting_enabled(database, "activity_browser_bridge_enabled") {
        return http_response(403, "activity tracking disabled");
    }
    let expected = match ensure_bridge_token(database) {
        Ok(token) => format!("Bearer {token}"),
        Err(_) => return http_response(500, "bridge unavailable"),
    };
    let authorized = lines
        .filter_map(|line| line.split_once(':'))
        .any(|(name, value)| {
            name.eq_ignore_ascii_case("authorization")
                && constant_time_eq(value.trim().as_bytes(), expected.as_bytes())
        });
    if !authorized {
        return http_response(401, "unauthorized");
    }
    let Ok(payload) = serde_json::from_str::<BrowserActivityPayload>(body) else {
        return http_response(400, "invalid payload");
    };
    if payload.source != "grok_web" {
        return http_response(400, "unsupported source");
    }
    let current_minute = Utc::now().timestamp().div_euclid(60);
    if payload.minute_epoch.abs_diff(current_minute) > MAX_MINUTE_DRIFT as u64 {
        return http_response(400, "minute outside accepted window");
    }
    let Some(event) = UsageEvent::activity(
        "grok",
        "grok_web",
        SourceType::Browser,
        payload.minute_epoch,
        device_id,
    ) else {
        return http_response(400, "invalid minute");
    };
    match database.insert_usage_events(&[event]) {
        Ok(_) => http_response(202, "accepted"),
        Err(_) => http_response(500, "could not save activity"),
    }
}

fn setting_enabled(database: &Database, key: &str) -> bool {
    database
        .setting(key)
        .ok()
        .flatten()
        .is_some_and(|value| value == "true")
}

fn request_is_complete(bytes: &[u8]) -> bool {
    let Ok(request) = std::str::from_utf8(bytes) else {
        return true;
    };
    let Some((headers, body)) = request.split_once("\r\n\r\n") else {
        return false;
    };
    let length = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find_map(|(name, value)| {
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    body.len() >= length
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(target_os = "macos")]
pub fn claude_desktop_is_frontmost() -> bool {
    use objc2_app_kit::NSWorkspace;
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|app| app.bundleIdentifier())
        .is_some_and(|bundle| bundle.to_string() == "com.anthropic.claudefordesktop")
}

#[cfg(not(target_os = "macos"))]
pub fn claude_desktop_is_frontmost() -> bool {
    false
}

pub fn record_claude_minute_if_active(
    database: &Database,
    device_id: &str,
) -> crate::db::Result<bool> {
    if !setting_enabled(database, "activity_claude_desktop_enabled")
        || !claude_desktop_is_frontmost()
    {
        return Ok(false);
    }
    let minute_epoch = Utc::now().timestamp().div_euclid(60);
    let Some(event) = UsageEvent::activity(
        "claude",
        "claude_desktop",
        SourceType::Manual,
        minute_epoch,
        device_id,
    ) else {
        return Ok(false);
    };
    Ok(database.insert_usage_events(&[event])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bridge_rejects_requests_until_explicitly_enabled() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path().join("activity.db")).unwrap();
        let response = process_request(
            b"POST /v1/activity HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}",
            &database,
            "device-1",
        );
        assert!(response.starts_with("HTTP/1.1 403"));
    }

    #[test]
    fn bridge_accepts_only_paired_current_grok_minutes_idempotently() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path().join("activity.db")).unwrap();
        database
            .set_setting("activity_browser_bridge_enabled", "true")
            .unwrap();
        let device = database.ensure_device("test").unwrap();
        let token = ensure_bridge_token(&database).unwrap();
        let minute = Utc::now().timestamp().div_euclid(60);
        let body = format!(r#"{{"source":"grok_web","minuteEpoch":{minute}}}"#);
        let request = format!(
            "POST /v1/activity HTTP/1.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        assert!(
            process_request(request.as_bytes(), &database, &device.id).starts_with("HTTP/1.1 202")
        );
        assert!(
            process_request(request.as_bytes(), &database, &device.id).starts_with("HTTP/1.1 202")
        );
        let connection = rusqlite::Connection::open(database.path()).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn status_reports_claude_desktop_minutes_without_token_claims() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path().join("activity.db")).unwrap();
        database
            .set_setting("activity_claude_desktop_enabled", "true")
            .unwrap();
        let device = database.ensure_device("test").unwrap();
        let minute = Utc::now().timestamp().div_euclid(60);
        let event = UsageEvent::activity(
            "claude",
            "claude_desktop",
            SourceType::Manual,
            minute,
            device.id,
        )
        .unwrap();
        let expected_time = event.occurred_at;
        assert_eq!(event.tokens.total_tokens, 0);
        database.insert_usage_events(&[event]).unwrap();

        let activity_status = status(&database).unwrap();
        assert!(activity_status.claude_desktop_enabled);
        assert_eq!(activity_status.claude_desktop_minutes, 1);
        assert_eq!(
            activity_status.claude_desktop_last_activity_at,
            Some(expected_time)
        );
    }
}
