use crate::AppState;
use crate::analytics::{self, ActivityItem, DashboardSnapshot};
use crate::auth::{self, AuthStatus};
use crate::collectors::{self, ScanReport};
use crate::db::Subscription;
use tauri::State;

#[tauri::command]
pub async fn dashboard_snapshot(
    range: String,
    state: State<'_, AppState>,
) -> Result<DashboardSnapshot, String> {
    let database = state.database.clone();
    let scan = state.last_scan.lock().await.clone();
    tauri::async_runtime::spawn_blocking(move || analytics::dashboard(&database, &range, &scan))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn scan_now(state: State<'_, AppState>) -> Result<ScanReport, String> {
    let _guard = state.scan_lock.lock().await;
    let database = state.database.clone();
    let device_id = state.device_id.clone();
    let report =
        tauri::async_runtime::spawn_blocking(move || collectors::scan_all(&database, &device_id))
            .await
            .map_err(|error| error.to_string())?;
    *state.last_scan.lock().await = report.clone();
    Ok(report)
}

#[tauri::command]
pub async fn activity_page(
    range: String,
    limit: i64,
    offset: i64,
    state: State<'_, AppState>,
) -> Result<Vec<ActivityItem>, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        analytics::activity_page(&database, &range, limit, offset)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_subscription(
    subscription: Subscription,
    state: State<'_, AppState>,
) -> Result<Vec<Subscription>, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.save_subscription(&subscription)?;
        database.subscriptions()
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn rename_device(
    name: String,
    state: State<'_, AppState>,
) -> Result<crate::device::Device, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.rename_device(&name))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_setting(
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.setting(&key))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_setting(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.set_setting(&key, &value))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn auth_status() -> AuthStatus {
    auth::status()
}

#[tauri::command]
pub async fn auth_sign_in(email: String, password: String) -> Result<AuthStatus, String> {
    auth::sign_in(&email, &password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn auth_sign_out() -> Result<AuthStatus, String> {
    auth::sign_out().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<crate::sync::SyncReport, String> {
    let database = state.database.clone();
    crate::sync::sync_now(&database)
        .await
        .map_err(|error| error.to_string())
}
