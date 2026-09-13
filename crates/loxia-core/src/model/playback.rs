//! PlaybackReport and PlayMethod.
//!
//! Lives in `loxia-core`, not `loxia-emby`, even though `loxia-emby::endpoints::playback::report`
//! is the only thing that sends one over the wire: `Effect::Net::ReportPlayback(PlaybackReport)`
//! is a pure `loxia-core` type, and `loxia-core` cannot depend on `loxia-emby` (the dependency
//! runs the other way). `loxia-emby` reuses this definition rather than declaring its own.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::ids::{ItemId, PlaySessionId};
use crate::config::QualityProfile;

/// `"DirectStream"` for the `Direct` quality profile, `"Transcode"` for every transcoding profile.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayMethod {
    DirectStream,
    Transcode,
}

impl PlayMethod {
    pub fn from_quality_profile(profile: QualityProfile) -> Self {
        match profile {
            QualityProfile::Direct => PlayMethod::DirectStream,
            QualityProfile::TranscodeHigh
            | QualityProfile::TranscodeMed
            | QualityProfile::TranscodeLow => PlayMethod::Transcode,
        }
    }

    /// The wire-format value Emby's `Sessions/Playing/Progress` expects.
    pub fn as_str(self) -> &'static str {
        match self {
            PlayMethod::DirectStream => "DirectStream",
            PlayMethod::Transcode => "Transcode",
        }
    }
}

/// Every call must be serialisable to the offline scrobble buffer (`08-07`), which is why this
/// carries plain owned data rather than borrowing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlaybackReport {
    Start {
        item: ItemId,
        session: PlaySessionId,
    },
    Progress {
        item: ItemId,
        session: PlaySessionId,
        position: Duration,
        paused: bool,
        play_method: PlayMethod,
    },
    Stopped {
        item: ItemId,
        session: PlaySessionId,
        position: Duration,
    },
    Played {
        item: ItemId,
    },
}
