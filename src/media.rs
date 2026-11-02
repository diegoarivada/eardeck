//! Pauses and resumes whatever Windows reports as the current media session.

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};

/// Returns true only if something was actually playing and got paused.
pub fn pause_if_playing() -> windows::core::Result<bool> {
    let session = Manager::RequestAsync()?.join()?.GetCurrentSession()?;
    if session.GetPlaybackInfo()?.PlaybackStatus()? != Status::Playing {
        return Ok(false);
    }
    session.TryPauseAsync()?.join()
}

pub fn resume() -> windows::core::Result<bool> {
    Manager::RequestAsync()?.join()?.GetCurrentSession()?.TryPlayAsync()?.join()
}
