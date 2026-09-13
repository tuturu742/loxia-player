//! Domain model types shared across the workspace.

pub mod audio_meta;
pub mod ids;
pub mod image;
pub mod item;
pub mod lyrics;
pub mod playback;

pub use audio_meta::{
    AudioDevice, AudioFormat, Codec, ReplayGainInfo, device_label, group_by_driver,
};
pub use ids::{
    ItemId, MediaSourceId, PlaySessionId, PlaylistEntryId, PlaylistId, QueueEntryId, ServerId,
    UserId,
};
pub use image::ImageSize;
pub use item::{
    Album, AlbumRelation, Artist, Folder, Genre, ImageRef, LyricFormat, LyricStreamRef, MediaItem,
    Playlist, SectionHeader, SectionKind, Track, format_now_playing,
};
pub use lyrics::{LyricLine, Lyrics};
pub use playback::{PlayMethod, PlaybackReport};
