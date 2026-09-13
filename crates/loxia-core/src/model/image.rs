//! ImageSize.
//!
//! Lives in `loxia-core`, not `loxia-emby`, because `Effect::Net::FetchImage{id, size, tag}` is a
//! pure `loxia-core` type and needs it too — `loxia-emby::endpoints::images` reuses this
//! definition rather than declaring its own (same reasoning as `model::playback`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageSize {
    /// 64px max height.
    Thumb,
    /// 600px max height.
    Large,
}

impl ImageSize {
    pub fn max_height(self) -> u32 {
        match self {
            ImageSize::Thumb => 64,
            ImageSize::Large => 600,
        }
    }

    pub fn cache_suffix(self) -> &'static str {
        match self {
            ImageSize::Thumb => "thumb",
            ImageSize::Large => "large",
        }
    }
}
