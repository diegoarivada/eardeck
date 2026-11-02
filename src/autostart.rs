//! Open at Login, kept in the per-user Run key so it never needs admin rights.

use windows_registry::CURRENT_USER;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "EarDeck";

/// The quotes matter: an unquoted path with spaces lets Windows run a planted
/// program from a parent folder instead.
fn command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\"", exe.display()))
}

pub fn is_enabled() -> bool {
    CURRENT_USER
        .open(RUN_KEY)
        .and_then(|key| key.get_string(VALUE))
        .is_ok()
}

pub fn set(enabled: bool) -> Result<(), String> {
    let key = CURRENT_USER.create(RUN_KEY).map_err(|e| e.message())?;
    if enabled {
        key.set_string(VALUE, &command()?).map_err(|e| e.message())
    } else if is_enabled() {
        key.remove_value(VALUE).map_err(|e| e.message())
    } else {
        Ok(())
    }
}

/// Rewrites an existing entry so it stays quoted and follows the app if it moves.
pub fn refresh() {
    if is_enabled() {
        let _ = set(true);
    }
}
