//! Pauses and resumes the media session that was playing on the AirPods.

use windows::core::HSTRING;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};

/// Pauses the current session if it is playing. Returns the app that was paused, so only
/// that app gets resumed later and not whatever happens to be current by then.
pub fn pause_if_playing() -> windows::core::Result<Option<HSTRING>> {
    let session = Manager::RequestAsync()?.join()?.GetCurrentSession()?;
    if session.GetPlaybackInfo()?.PlaybackStatus()? != Status::Playing {
        return Ok(None);
    }
    let app = session.SourceAppUserModelId()?;
    Ok(session.TryPauseAsync()?.join()?.then_some(app))
}

/// Resumes the app paused earlier, but only if it is still paused. If the user already
/// pressed play, or closed it, nothing happens.
pub fn resume(app: &HSTRING) -> windows::core::Result<bool> {
    for session in Manager::RequestAsync()?.join()?.GetSessions()? {
        if session.SourceAppUserModelId()? == *app {
            if session.GetPlaybackInfo()?.PlaybackStatus()? != Status::Paused {
                return Ok(false);
            }
            return session.TryPlayAsync()?.join();
        }
    }
    Ok(false)
}
