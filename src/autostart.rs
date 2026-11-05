//! Open at Login. The installer build uses the per-user Run key so it never needs admin
//! rights. The Microsoft Store (MSIX) build can't rely on that, because the package path
//! changes on every update, so it uses the StartupTask declared in its AppxManifest.xml.

use windows::ApplicationModel::{Package, StartupTask, StartupTaskState};
use windows_registry::CURRENT_USER;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "EarDeck";
/// Must match the TaskId in store/msix/AppxManifest.xml.
const TASK_ID: &str = "EarDeckStartup";

/// `Package::Current` only succeeds when running from an MSIX package.
fn packaged() -> bool {
    Package::Current().is_ok()
}

fn task() -> Result<StartupTask, String> {
    StartupTask::GetAsync(&TASK_ID.into())
        .and_then(|op| op.join())
        .map_err(|e| e.message())
}

/// The quotes matter: an unquoted path with spaces lets Windows run a planted
/// program from a parent folder instead.
fn command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\"", exe.display()))
}

pub fn is_enabled() -> bool {
    if packaged() {
        return task()
            .and_then(|t| t.State().map_err(|e| e.message()))
            .is_ok_and(|s| s == StartupTaskState::Enabled || s == StartupTaskState::EnabledByPolicy);
    }
    CURRENT_USER
        .open(RUN_KEY)
        .and_then(|key| key.get_string(VALUE))
        .is_ok()
}

fn set_task(enabled: bool) -> Result<(), String> {
    let task = task()?;
    if !enabled {
        return task.Disable().map_err(|e| e.message());
    }
    let state = task.RequestEnableAsync().and_then(|op| op.join()).map_err(|e| e.message())?;
    match state {
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => Ok(()),
        // Windows doesn't let an app undo the user's own choice in Settings.
        StartupTaskState::DisabledByUser => {
            Err("Turn EarDeck back on in Settings > Apps > Startup.".into())
        }
        _ => Err("Windows didn't allow Open at Login.".into()),
    }
}

pub fn set(enabled: bool) -> Result<(), String> {
    if packaged() {
        return set_task(enabled);
    }
    let key = CURRENT_USER.create(RUN_KEY).map_err(|e| e.message())?;
    if enabled {
        key.set_string(VALUE, &command()?).map_err(|e| e.message())
    } else if is_enabled() {
        key.remove_value(VALUE).map_err(|e| e.message())
    } else {
        Ok(())
    }
}

/// Rewrites an existing Run entry so it stays quoted and follows the app if it moves.
/// The packaged build has nothing to refresh: Windows tracks the startup task itself.
pub fn refresh() {
    if !packaged() && is_enabled() {
        let _ = set(true);
    }
}
