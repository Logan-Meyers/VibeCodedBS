pub mod cleaner;
pub mod providers;

use std::path::PathBuf;

pub use providers::{TrackMetadata, TrackQuery};

/// Reads basic track info from a file path (filename-based fallback)
pub fn query_from_path(path: &PathBuf) -> TrackQuery {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Try "Artist - Title" format
    if let Some((artist, title)) = stem.split_once(" - ") {
        TrackQuery {
            artist: Some(artist.trim().to_string()),
            title: Some(title.trim().to_string()),
            ..Default::default()
        }
    } else {
        TrackQuery {
            title: Some(stem),
            ..Default::default()
        }
    }
}
