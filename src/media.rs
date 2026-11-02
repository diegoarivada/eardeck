//! Pauses and resumes whatever Windows reports as the current media session.

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};

/// Returns true only if something was actually playing and got paused.
pub fn pause_if_playing() -> windows::core::Result<bool> {
    let session = Manager::RequestAsync()?.get()?.GetCurrentSession()?;
    if session.GetPlaybackInfo()?.PlaybackStatus()? != Status::Playing {
        return Ok(false);
    }
    session.TryPauseAsync()?.get()
}

pub fn resume() -> windows::core::Result<bool> {
    Manager::RequestAsync()?.get()?.GetCurrentSession()?.TryPlayAsync()?.get()
}
