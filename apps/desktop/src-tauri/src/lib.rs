#![cfg_attr(test, allow(dead_code))]

mod activity_tracking;
mod analytics;
mod auth;
mod collectors;
#[cfg(not(test))]
mod commands;
mod db;
mod device;
mod domain;
mod pricing;
mod sync;

#[cfg(not(test))]
use analytics::DashboardSnapshot;
#[cfg(not(test))]
use collectors::ScanReport;
#[cfg(not(test))]
use db::Database;
#[cfg(not(test))]
use std::sync::Arc;
#[cfg(not(test))]
use tauri::menu::{Menu, MenuItem};
#[cfg(not(test))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
#[cfg(not(test))]
use tauri::{Emitter, Manager, WindowEvent};
#[cfg(not(test))]
use tauri_plugin_autostart::MacosLauncher;
#[cfg(not(test))]
use tokio::sync::Mutex;

#[cfg(not(test))]
pub struct AppState {
    database: Database,
    device_id: String,
    last_scan: Arc<Mutex<ScanReport>>,
    scan_lock: Arc<Mutex<()>>,
}

#[cfg(not(test))]
struct TraySummary {
    today: MenuItem<tauri::Wry>,
    codex: MenuItem<tauri::Wry>,
    claude: MenuItem<tauri::Wry>,
    grok: MenuItem<tauri::Wry>,
    gemini: MenuItem<tauri::Wry>,
    month: MenuItem<tauri::Wry>,
}

#[cfg(not(test))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            let database = Database::open(app_data.join("arcmeter.db"))?;
            let device = database.ensure_device(app.package_info().version.to_string().as_str())?;
            database.ensure_default_subscriptions()?;
            let _ = activity_tracking::ensure_bridge_token(&database)?;

            app.manage(AppState {
                database: database.clone(),
                device_id: device.id.clone(),
                last_scan: Arc::new(Mutex::new(ScanReport::default())),
                scan_lock: Arc::new(Mutex::new(())),
            });

            build_tray(app)?;

            activity_tracking::start_browser_bridge(database.clone(), device.id.clone());
            let activity_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(state) = activity_handle.try_state::<AppState>() {
                    let database = state.database.clone();
                    let device_id = state.device_id.clone();
                    if let Ok(Ok(inserted)) = tauri::async_runtime::spawn_blocking(move || {
                        activity_tracking::record_claude_minute_if_active(&database, &device_id)
                    })
                    .await
                        && inserted
                    {
                        let _ = activity_handle.emit("arcmeter://data-changed", ());
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                }
            });

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(state) = app_handle.try_state::<AppState>() {
                    let guard = state.scan_lock.lock().await;
                    let database = state.database.clone();
                    let device_id = state.device_id.clone();
                    if let Ok(Ok((report, today, month))) =
                        tauri::async_runtime::spawn_blocking(move || {
                            collect_and_summarize(&database, &device_id)
                        })
                        .await
                    {
                        *state.last_scan.lock().await = report;
                        update_tray_summary(&app_handle, &today, &month);
                        let _ = app_handle.emit("arcmeter://data-changed", ());
                    }
                    drop(guard);
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            });
            let sync_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let policy = sync::RetryPolicy::default();
                let mut failures = 0_u32;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                    let Some(state) = sync_handle.try_state::<AppState>() else {
                        continue;
                    };
                    if !auth::status().signed_in {
                        continue;
                    }
                    match sync::sync_now(&state.database).await {
                        Ok(_) => {
                            failures = 0;
                            let _ = sync_handle.emit("arcmeter://data-changed", ());
                        }
                        Err(_) => {
                            failures = failures.saturating_add(1);
                            tokio::time::sleep(policy.delay(failures)).await;
                        }
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                let close_to_tray = window
                    .try_state::<AppState>()
                    .and_then(|state| state.database.setting("close_to_tray").ok().flatten())
                    .is_none_or(|value| value == "true");
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::dashboard_snapshot,
            commands::scan_now,
            commands::activity_page,
            commands::save_subscription,
            commands::rename_device,
            commands::get_setting,
            commands::set_setting,
            commands::activity_tracking_status,
            commands::auth_status,
            commands::auth_sign_in,
            commands::auth_sign_out,
            commands::sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ArcMeter");
}

#[cfg(not(test))]
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let today = MenuItem::with_id(
        app,
        "today_summary",
        "Today · collecting usage…",
        false,
        None::<&str>,
    )?;
    let codex = MenuItem::with_id(app, "codex_summary", "Codex · —", false, None::<&str>)?;
    let claude = MenuItem::with_id(app, "claude_summary", "Claude · —", false, None::<&str>)?;
    let grok = MenuItem::with_id(app, "grok_summary", "Grok · —", false, None::<&str>)?;
    let gemini = MenuItem::with_id(app, "gemini_summary", "Gemini · —", false, None::<&str>)?;
    let month = MenuItem::with_id(
        app,
        "month_summary",
        "This month · local-first",
        false,
        None::<&str>,
    )?;
    let open = MenuItem::with_id(app, "open", "Open ArcMeter", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Sync Now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &today, &codex, &claude, &grok, &gemini, &month, &open, &sync, &quit,
        ],
    )?;

    TrayIconBuilder::new()
        .tooltip("ArcMeter")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "sync" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let Some(state) = app.try_state::<AppState>() else {
                        return;
                    };
                    if sync::sync_now(&state.database).await.is_ok() {
                        let _ = app.emit("arcmeter://data-changed", ());
                    }
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(TraySummary {
        today,
        codex,
        claude,
        grok,
        gemini,
        month,
    });
    Ok(())
}

#[cfg(not(test))]
fn collect_and_summarize(
    database: &Database,
    device_id: &str,
) -> db::Result<(ScanReport, DashboardSnapshot, DashboardSnapshot)> {
    let report = collectors::scan_all(database, device_id);
    let today = analytics::dashboard(database, "today", &report)?;
    let month = analytics::dashboard(database, "month", &report)?;
    Ok((report, today, month))
}

#[cfg(not(test))]
fn update_tray_summary(
    app: &tauri::AppHandle,
    today: &DashboardSnapshot,
    month: &DashboardSnapshot,
) {
    let Some(tray) = app.try_state::<TraySummary>() else {
        return;
    };
    let _ = tray.today.set_text(format!(
        "Today · {} measured tokens",
        compact_tokens(today.metrics.measured_tokens_today)
    ));
    let _ = tray
        .codex
        .set_text(provider_summary(today, "codex", "Codex"));
    let _ = tray
        .claude
        .set_text(provider_summary(today, "claude", "Claude"));
    let _ = tray.grok.set_text(provider_summary(today, "grok", "Grok"));
    let _ = tray
        .gemini
        .set_text(provider_summary(today, "gemini", "Gemini"));
    let _ = tray.month.set_text(format!(
        "This month · {} measured tokens",
        compact_tokens(month.metrics.measured_tokens_month)
    ));
}

#[cfg(not(test))]
fn provider_summary(snapshot: &DashboardSnapshot, provider: &str, label: &str) -> String {
    let tokens = snapshot
        .by_provider
        .iter()
        .find(|item| item.key == provider)
        .map_or(0, |item| item.tokens);
    format!("{label} · {}", compact_tokens(tokens))
}

#[cfg(not(test))]
fn compact_tokens(tokens: i64) -> String {
    let tokens = tokens.max(0) as f64;
    if tokens >= 1_000_000_000.0 {
        format!("{:.1}B", tokens / 1_000_000_000.0)
    } else if tokens >= 1_000_000.0 {
        format!("{:.1}M", tokens / 1_000_000.0)
    } else if tokens >= 1_000.0 {
        format!("{:.1}K", tokens / 1_000.0)
    } else {
        format!("{}", tokens as i64)
    }
}

#[cfg(not(test))]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
