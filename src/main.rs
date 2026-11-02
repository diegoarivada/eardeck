#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod autostart;
mod ble;
mod media;
mod monitor;
mod pairing;
mod parser;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use monitor::{App, Settings, Status};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::utils::config::WindowEffectsConfig;
use tauri::utils::{WindowEffect, WindowEffectState};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Rect, RunEvent, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

const WIDTH: f64 = 340.0;
const HEIGHT: f64 = 560.0;
const EDGE_GAP: f64 = 12.0;
const AUTO_HIDE: Duration = Duration::from_secs(6);
/// Clicking the tray blurs the popup first; without this the click would reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

static LAST_HIDE: Mutex<Option<Instant>> = Mutex::new(None);

fn popup(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

fn place(win: &WebviewWindow, anchor: Option<Rect>) {
    let scale = win.scale_factor().unwrap_or(1.0);
    let Ok(size) = win.outer_size() else { return };
    let (w, h) = (size.width as f64, size.height as f64);
    let gap = EDGE_GAP * scale;

    let tray = anchor.map(|r| {
        let p = r.position.to_physical::<f64>(scale);
        let s = r.size.to_physical::<f64>(scale);
        (p.x + s.width / 2.0, p.y)
    });
    let monitor = tray
        .and_then(|(x, y)| win.monitor_from_point(x, y).ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let area = monitor.work_area();
    let (left, top) = (area.position.x as f64, area.position.y as f64);
    let (right, bottom) = (left + area.size.width as f64, top + area.size.height as f64);

    let x = tray.map_or(right - w - gap, |(cx, _)| cx - w / 2.0);
    let x = x.clamp(left + gap, (right - w - gap).max(left + gap));
    let _ = win.set_position(PhysicalPosition::new(x.round() as i32, (bottom - h - gap).round() as i32));
}

/// The web view is the only heavy part of the app, so it exists only while the popup is open.
/// Runs on its own thread because building a window from a UI callback deadlocks on Windows.
fn open(app: &AppHandle, anchor: Option<Rect>, take_focus: bool) {
    let app = app.clone();
    std::thread::spawn(move || {
        let acrylic = WindowEffectsConfig {
            effects: vec![WindowEffect::Acrylic],
            state: Some(WindowEffectState::Active),
            ..Default::default()
        };
        let built = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
            .title("EarDeck")
            .inner_size(WIDTH, HEIGHT)
            .visible(false)
            .focused(take_focus)
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .transparent(true)
            .shadow(true)
            .effects(acrylic)
            // The popup only ever shows the pages bundled inside the app.
            .on_navigation(|url| url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost"))
            .build();
        let Ok(win) = built else { return };
        place(&win, anchor);
        if !take_focus {
            std::thread::sleep(AUTO_HIDE);
            if !win.is_focused().unwrap_or(false) {
                let _ = win.destroy();
            }
        }
    });
}

fn toggle(app: &AppHandle, anchor: Rect) {
    if let Some(win) = popup(app) {
        let _ = win.destroy();
    } else if !LAST_HIDE.lock().unwrap().is_some_and(|t| t.elapsed() < REOPEN_GUARD) {
        open(app, Some(anchor), true);
    }
}

/// Shown on connect: appears without stealing focus, then leaves on its own.
pub fn show_popup(app: &AppHandle) {
    if popup(app).is_none() {
        open(app, None, false);
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

/// The settings file is user-writable, so nothing in it is trusted to be in range.
fn sanitize(settings: Settings) -> Settings {
    Settings { low_battery: settings.low_battery.min(100), ..settings }
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .map(sanitize)
        .unwrap_or_default()
}

pub fn write_settings(app: &AppHandle, settings: &Settings) {
    let Some(path) = settings_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(settings) {
        let _ = std::fs::write(path, json);
    }
}

fn endpoint(state: &Arc<App>) -> Result<String, String> {
    state.endpoint_id.lock().unwrap().clone().ok_or_else(|| "No AirPods found".to_string())
}

/// The page calls this once it has painted, so the popup never flashes empty.
#[tauri::command]
fn ui_ready(window: WebviewWindow) {
    let _ = window.show();
}

#[tauri::command]
fn get_status(state: State<'_, Arc<App>>) -> Status {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
fn get_settings(state: State<'_, Arc<App>>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<'_, Arc<App>>, settings: Settings) {
    let mut current = state.settings.lock().unwrap();
    // The device pick lives in the backend; the toggles page does not own it.
    let selected = current.selected.clone();
    *current = sanitize(Settings { selected, ..settings });
    write_settings(&app, &current);
}

#[tauri::command]
fn select_device(app: AppHandle, state: State<'_, Arc<App>>, id: String) -> Result<(), String> {
    if !state.status.lock().unwrap().devices.iter().any(|d| d.id == id) {
        return Err("That device isn't paired".into());
    }
    let mut settings = state.settings.lock().unwrap();
    settings.selected = Some(id);
    write_settings(&app, &settings);
    state.refresh.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command(async)]
fn start_pair_scan(app: AppHandle, state: State<'_, Arc<App>>) -> Result<(), String> {
    stop_scan(&state);
    let handle = app.clone();
    let watcher = pairing::start_scan(move |found| {
        let _ = handle.emit("pair-found", found);
    })
    .map_err(|e| e.message())?;
    *state.scan.lock().unwrap() = Some(watcher);
    Ok(())
}

#[tauri::command(async)]
fn stop_pair_scan(state: State<'_, Arc<App>>) {
    stop_scan(&state);
}

fn stop_scan(state: &Arc<App>) {
    if let Some(watcher) = state.scan.lock().unwrap().take() {
        let _ = watcher.Stop();
    }
}

#[tauri::command(async)]
fn pair_device(state: State<'_, Arc<App>>, id: String, name: String) -> Result<(), String> {
    stop_scan(&state);
    state.pairing.store(true, Ordering::Relaxed);
    let result = pairing::pair(&id);
    state.pairing.store(false, Ordering::Relaxed);
    result?;
    *state.pending.lock().unwrap() = Some((name, Instant::now()));
    Ok(())
}

#[tauri::command]
fn open_bluetooth_settings() {
    let _ = std::process::Command::new("explorer").arg("ms-settings:bluetooth").spawn();
}

#[tauri::command]
fn get_autostart() -> bool {
    autostart::is_enabled()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    autostart::set(enabled)
}

#[tauri::command(async)]
fn connect(state: State<'_, Arc<App>>) -> Result<(), String> {
    audio::connect(&endpoint(&state)?)
}

#[tauri::command(async)]
fn disconnect(state: State<'_, Arc<App>>) -> Result<(), String> {
    audio::disconnect(&endpoint(&state)?)
}

#[tauri::command(async)]
fn make_default(state: State<'_, Arc<App>>) -> Result<(), String> {
    audio::set_default(&endpoint(&state)?)
}

#[tauri::command(async)]
fn set_volume(state: State<'_, Arc<App>>, percent: u8) -> Result<(), String> {
    audio::set_volume(&endpoint(&state)?, percent)
}

#[tauri::command]
fn hide_popup(app: AppHandle) {
    if let Some(win) = popup(&app) {
        let _ = win.destroy();
    }
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn main() {
    // This variable can open a remote debug port into the web view. Nothing legitimate needs it.
    #[cfg(not(debug_assertions))]
    std::env::remove_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_popup(app)))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            ui_ready, get_status, get_settings, save_settings, get_autostart, set_autostart,
            connect, disconnect, make_default, set_volume, hide_popup, quit,
            select_device, start_pair_scan, stop_pair_scan, pair_device, open_bluetooth_settings
        ])
        .on_window_event(|win, event| {
            let state = win.state::<Arc<App>>();
            match event {
                WindowEvent::Focused(false) => {
                    // Windows can steal focus mid-pairing; the card has to survive that.
                    let busy = state.scan.lock().unwrap().is_some()
                        || state.pairing.load(Ordering::Relaxed);
                    if !busy {
                        *LAST_HIDE.lock().unwrap() = Some(Instant::now());
                        let _ = win.destroy();
                    }
                }
                WindowEvent::Destroyed => stop_scan(&state),
                _ => {}
            }
        })
        .setup(|app| {
            autostart::refresh();
            let handle = app.handle().clone();
            let state = Arc::new(App {
                status: Mutex::new(Status::default()),
                settings: Mutex::new(load_settings(&handle)),
                endpoint_id: Mutex::new(None),
                refresh: AtomicBool::new(false),
                pending: Mutex::new(None),
                scan: Mutex::new(None),
                pairing: AtomicBool::new(false),
            });
            app.manage(state.clone());

            let open_item = MenuItemBuilder::with_id("open", "Open EarDeck").build(app)?;
            let connect = MenuItemBuilder::with_id("connect", "Connect").build(app)?;
            let disconnect = MenuItemBuilder::with_id("disconnect", "Disconnect").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&open_item, &connect, &disconnect])
                .separator()
                .item(&quit)
                .build()?;

            let icon = app.default_window_icon().cloned().ok_or("Missing app icon")?;
            let tray_state = state.clone();
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("EarDeck")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "open" => {
                        if popup(app).is_none() {
                            open(app, None, true);
                        }
                    }
                    "quit" => app.exit(0),
                    action => {
                        let Ok(id) = endpoint(&tray_state) else { return };
                        let connecting = action == "connect";
                        std::thread::spawn(move || {
                            let _ = if connecting { audio::connect(&id) } else { audio::disconnect(&id) };
                        });
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        toggle(tray.app_handle(), rect);
                    }
                })
                .build(app)?;

            monitor::spawn(handle, state);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start EarDeck")
        .run(|_, event| {
            // Closing the popup closes the last window; the tray has to keep running.
            if let RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}
