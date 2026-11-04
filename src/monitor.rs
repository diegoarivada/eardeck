//! Background loop: watches the headphones and drives auto-pause, alerts and the UI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use windows::Devices::Enumeration::DeviceWatcher;

use crate::parser::{Form, Reading};
use crate::{audio, ble, media};

const POLL: Duration = Duration::from_secs(1);
const IDLE_POLL: Duration = Duration::from_secs(5);
const ENDPOINT_REFRESH_SECS: u64 = 30;
const WEAR_CONFIRM_POLLS: u8 = 2;
const WEAR_FRESH_SECS: u64 = 10;
const PENDING_TIMEOUT_SECS: u64 = 30;
/// A battery level older than this may be from before a charge, so it never raises an alert.
const ALERT_FRESH_SECS: u64 = 60;

/// Just-paired headphones, waiting for Windows to add their audio endpoint.
#[derive(Clone, Debug)]
pub struct Pending {
    pub name: String,
    /// Endpoints that existed before pairing, so an older pair with a similar name is not picked.
    pub known: Vec<String>,
    pub started: Instant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub auto_pause: bool,
    pub auto_default: bool,
    pub popup_on_connect: bool,
    pub low_battery: u8,
    /// Endpoint id of the headphones picked in My Devices.
    pub selected: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_pause: true,
            auto_default: true,
            popup_on_connect: true,
            low_battery: 20,
            selected: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub connected: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub found: bool,
    pub id: Option<String>,
    pub name: String,
    pub connected: bool,
    pub is_default: bool,
    pub volume: Option<u8>,
    pub reading: Option<Reading>,
    pub seen_secs_ago: Option<u64>,
    pub worn: Option<bool>,
    pub devices: Vec<Device>,
}

pub struct App {
    pub status: Mutex<Status>,
    pub settings: Mutex<Settings>,
    pub endpoint_id: Mutex<Option<String>>,
    pub refresh: AtomicBool,
    pub pending: Mutex<Option<Pending>>,
    pub scan: Mutex<Option<DeviceWatcher>>,
    pub pairing: AtomicBool,
}

fn clean(s: &str) -> String {
    s.replace('\u{2019}', "'").trim().to_lowercase()
}

/// The endpoint Windows made for just-paired headphones. A new endpoint with the exact name
/// wins, then any new one with the name inside it, then an existing exact match (already paired).
fn match_new<'a>(list: &'a [audio::Endpoint], pending: &Pending) -> Option<&'a audio::Endpoint> {
    let want = clean(&pending.name);
    if want.is_empty() {
        return None;
    }
    let is_new = |e: &audio::Endpoint| !pending.known.contains(&e.id);
    let exact = |e: &audio::Endpoint| clean(&audio::display_name(&e.name)) == want;
    list.iter()
        .find(|e| is_new(e) && exact(e))
        .or_else(|| list.iter().find(|e| is_new(e) && clean(&e.name).contains(&want)))
        .or_else(|| list.iter().find(|e| exact(e)))
}

/// Picked device first, then anything connected. Otherwise stay on the current one, so a
/// disconnect does not jump the app to a different pair.
fn choose(list: &[audio::Endpoint], selected: Option<&str>, current: Option<&str>) -> Option<audio::Endpoint> {
    let by_id = |id: Option<&str>| list.iter().find(|e| Some(e.id.as_str()) == id);
    by_id(selected)
        .or_else(|| list.iter().find(|e| e.active))
        .or_else(|| by_id(current))
        .or_else(|| list.first())
        .cloned()
}

fn worn(r: &Reading) -> bool {
    match r.form {
        Form::Single => r.left_in_ear || r.right_in_ear,
        Form::Buds => r.left_in_ear && r.right_in_ear,
    }
}

fn lowest_battery(r: &Reading) -> Option<(u8, bool)> {
    [
        (r.main, r.main_charging),
        (r.left, r.left_charging),
        (r.right, r.right_charging),
    ]
    .into_iter()
    .filter_map(|(level, charging)| level.map(|l| (l, charging)))
    .min_by_key(|(l, _)| *l)
}

fn tooltip(s: &Status) -> String {
    if !s.found {
        return "EarDeck: no AirPods paired".into();
    }
    let state = if s.connected { "connected" } else { "not connected" };
    match s.reading.as_ref().and_then(lowest_battery) {
        Some((level, _)) => format!("{}: {level}%, {state}", s.name),
        None => format!("{}: {state}", s.name),
    }
}

pub fn spawn(app: AppHandle, state: Arc<App>) {
    std::thread::spawn(move || run(app, state));
}

fn run(app: AppHandle, state: Arc<App>) {
    audio::com_init();
    let shared = ble::Shared::default();
    let scanner = ble::Scanner::new(shared.clone()).ok();
    let mut wanted = true;

    let mut endpoint: Option<audio::Endpoint> = None;
    let mut devices: Vec<Device> = vec![];
    let mut last_refresh: Option<Instant> = None;
    let mut was_connected = false;
    let mut first_pass = true;
    let mut settled_worn: Option<bool> = None;
    let mut wear_votes = 0u8;
    let mut paused_app: Option<windows::core::HSTRING> = None;
    let mut low_alerted = false;
    let mut last_sent = Status::default();

    loop {
        if let Some(scanner) = &scanner {
            scanner.set_running(wanted);
        }

        let pending = state.pending.lock().unwrap().clone();
        let stale = last_refresh.is_none_or(|t| t.elapsed().as_secs() >= ENDPOINT_REFRESH_SECS);
        if endpoint.is_none() || stale || pending.is_some() || state.refresh.swap(false, Ordering::Relaxed) {
            let list = audio::all_headphones();
            devices = list
                .iter()
                .map(|e| Device { id: e.id.clone(), name: audio::display_name(&e.name), connected: e.active })
                .collect();

            if let Some(pending) = pending {
                if let Some(new) = match_new(&list, &pending) {
                    *state.pending.lock().unwrap() = None;
                    {
                        let mut settings = state.settings.lock().unwrap();
                        settings.selected = Some(new.id.clone());
                        crate::write_settings(&app, &settings);
                    }
                    let _ = audio::connect(&new.id);
                } else if pending.started.elapsed().as_secs() >= PENDING_TIMEOUT_SECS {
                    *state.pending.lock().unwrap() = None;
                }
            }

            let selected = state.settings.lock().unwrap().selected.clone();
            let current = endpoint.as_ref().map(|e| e.id.clone());
            if let Some(found) = choose(&list, selected.as_deref(), current.as_deref()) {
                let name = audio::display_name(&found.name);
                let mut current = shared.device_name.lock().unwrap();
                if *current != name {
                    *current = name;
                    // Battery from the previous headphones must not show under the new name.
                    *shared.latest.lock().unwrap() = None;
                    was_connected = false;
                    // Wear, pause and alert state belonged to the previous headphones too.
                    settled_worn = None;
                    wear_votes = 0;
                    paused_app = None;
                    low_alerted = false;
                }
                *state.endpoint_id.lock().unwrap() = Some(found.id.clone());
                endpoint = Some(found);
            }
            last_refresh = Some(Instant::now());
        }

        let settings = state.settings.lock().unwrap().clone();
        let mut status = Status { devices: devices.clone(), ..Status::default() };

        if let Some(e) = &endpoint {
            status.found = true;
            status.id = Some(e.id.clone());
            status.name = audio::display_name(&e.name);
            status.connected = audio::is_active(&e.id);
            // The list only refreshes every few seconds; the current device is known now.
            if let Some(d) = status.devices.iter_mut().find(|d| d.id == e.id) {
                d.connected = status.connected;
            }
            status.is_default = status.connected && audio::default_id().as_deref() == Some(&e.id);
            if status.connected {
                status.volume = audio::get_volume(&e.id);
            }

            // Already connected at startup is not a new connection, so leave audio where it is.
            if status.connected && !was_connected && !first_pass {
                if settings.auto_default {
                    let _ = audio::set_default(&e.id);
                }
                if settings.popup_on_connect {
                    crate::show_popup(&app);
                }
            }
            was_connected = status.connected;
            first_pass = false;
        }

        // Without paired headphones there is nothing to tie a broadcast to.
        let seen = if status.found { shared.latest.lock().unwrap().clone() } else { None };
        if let Some(seen) = seen {
            let age = seen.at.elapsed().as_secs();
            status.seen_secs_ago = Some(age);
            if age < WEAR_FRESH_SECS {
                status.worn = Some(worn(&seen.reading));
            }
            status.reading = Some(seen.reading);
        }

        // Packets can glitch, so a wear change has to hold before it counts.
        match (status.worn, settled_worn) {
            (Some(now), Some(before)) if now != before => {
                wear_votes += 1;
                if wear_votes >= WEAR_CONFIRM_POLLS {
                    wear_votes = 0;
                    settled_worn = Some(now);
                    if settings.auto_pause && status.connected && status.is_default {
                        if !now {
                            paused_app = media::pause_if_playing().ok().flatten();
                        } else if let Some(app) = paused_app.take() {
                            let _ = media::resume(&app);
                        }
                    }
                }
            }
            (Some(now), None) => settled_worn = Some(now),
            _ => wear_votes = 0,
        }
        // A pause from before a disconnect must not resume music hours later.
        if !status.connected {
            paused_app = None;
            settled_worn = None;
            wear_votes = 0;
        }

        let fresh = status.seen_secs_ago.is_some_and(|age| age < ALERT_FRESH_SECS);
        if let Some((level, charging)) = status.reading.as_ref().filter(|_| fresh).and_then(lowest_battery) {
            if level <= settings.low_battery && !charging && status.connected && !low_alerted {
                low_alerted = true;
                let _ = app
                    .notification()
                    .builder()
                    .title(format!("{} is running low", status.name))
                    .body(format!("{level}% battery left"))
                    .show();
            } else if level > settings.low_battery.saturating_add(10) || charging {
                low_alerted = false;
            }
        }

        // Age ticks every poll, so compare without it to avoid needless UI updates.
        let changed = {
            let (mut a, mut b) = (status.clone(), last_sent.clone());
            a.seen_secs_ago = a.seen_secs_ago.map(|s| s / 30);
            b.seen_secs_ago = b.seen_secs_ago.map(|s| s / 30);
            a != b
        };
        if changed {
            last_sent = status.clone();
            let _ = app.emit("status", &status);
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_tooltip(Some(tooltip(&status)));
            }
        }
        // Nothing to watch closely while disconnected, so the loop and the radio both rest.
        wanted = status.connected || app.get_webview_window("main").is_some();
        *state.status.lock().unwrap() = status;

        std::thread::sleep(if wanted { POLL } else { IDLE_POLL });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(id: &str, name: &str, active: bool) -> audio::Endpoint {
        audio::Endpoint { id: id.into(), name: name.into(), active, bluetooth: true }
    }

    fn pending(name: &str, known: &[&str]) -> Pending {
        Pending { name: name.into(), known: known.iter().map(|s| s.to_string()).collect(), started: Instant::now() }
    }

    #[test]
    fn new_pair_does_not_grab_an_older_pair_with_a_similar_name() {
        let list = [
            ep("old", "Headphones (Sam's AirPods Pro)", true),
            ep("new", "Headphones (AirPods Pro)", false),
        ];
        assert_eq!(match_new(&list, &pending("AirPods Pro", &["old"])).map(|e| e.id.as_str()), Some("new"));
    }

    #[test]
    fn waits_until_windows_adds_the_endpoint() {
        let list = [ep("old", "Headphones (Sam's AirPods Pro)", true)];
        assert!(match_new(&list, &pending("AirPods Pro", &["old"])).is_none());
    }

    #[test]
    fn already_paired_headphones_are_found_by_exact_name() {
        let list = [ep("a", "Headphones (Sam\u{2019}s AirPods - Find My)", false)];
        assert_eq!(match_new(&list, &pending("Sam's AirPods", &["a"])).map(|e| e.id.as_str()), Some("a"));
    }

    #[test]
    fn empty_name_matches_nothing() {
        let list = [ep("a", "Headphones (AirPods)", false)];
        assert!(match_new(&list, &pending("  ", &[])).is_none());
    }

    #[test]
    fn choose_prefers_pick_then_connected_then_current() {
        let list = [ep("a", "A", false), ep("b", "B", true), ep("c", "C", false)];
        assert_eq!(choose(&list, Some("c"), None).unwrap().id, "c");
        assert_eq!(choose(&list, Some("gone"), Some("a")).unwrap().id, "b");
        let idle = [ep("a", "A", false), ep("c", "C", false)];
        assert_eq!(choose(&idle, None, Some("c")).unwrap().id, "c");
        assert!(choose(&[], None, None).is_none());
    }

    #[test]
    fn lowest_battery_ignores_the_case() {
        let r = crate::parser::parse(&{
            let mut p = vec![0u8; 27];
            p[0] = 0x07;
            [p[3], p[4]] = [0x14, 0x20];
            p[5] = 0x20;
            p[6] = 0x73;
            p[7] = 0x01;
            p
        })
        .unwrap();
        assert_eq!(r.case, Some(10));
        assert_eq!(lowest_battery(&r), Some((30, false)));
    }
}
