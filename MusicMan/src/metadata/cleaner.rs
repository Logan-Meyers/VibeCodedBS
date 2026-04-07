use anyhow::Result;
use audiotags::Tag;
use std::path::PathBuf;
use std::time::SystemTime;

/// Strip all tags, keep only: title, artist, track_number, year
pub fn clean_file(path: &PathBuf) -> Result<()> {
    let tag = Tag::new().read_from_path(path);
    let Ok(mut tag) = tag else {
        tracing::warn!("clean_file: could not read tags for {}", path.display());
        return Ok(());
    };

    let title        = tag.title().map(String::from);
    let artist       = tag.artist().map(String::from);
    let track_number = tag.track_number();
    let year         = tag.year();

    tag.remove_album_cover();

    if let Some(t) = &title  { tag.set_title(t); }   else { tag.remove_title(); }
    if let Some(a) = &artist { tag.set_artist(a); }  else { tag.remove_artist(); }
    if let Some(n) = track_number { tag.set_track_number(n); }
    if let Some(y) = year        { tag.set_year(y); }

    tag.write_to_path(path.to_str().unwrap_or(""))?;
    Ok(())
}

/// Apply fetched metadata to a file, writing only essential tags
pub fn apply_metadata(path: &PathBuf, meta: &crate::metadata::TrackMetadata) -> Result<()> {
    let Ok(mut tag) = Tag::new().read_from_path(path) else {
        tracing::warn!("apply_metadata: could not read tags for {}", path.display());
        return Ok(());
    };

    if let Some(t) = &meta.title        { tag.set_title(t); }
    if let Some(a) = &meta.artist       { tag.set_artist(a); }
    if let Some(n) = meta.track_number  { tag.set_track_number(n as u16); }
    if let Some(y) = meta.year          { tag.set_year(y as i32); }

    tag.write_to_path(path.to_str().unwrap_or(""))?;
    Ok(())
}

/// Write album art bytes into a file's tags
pub fn apply_album_art(path: &PathBuf, art_bytes: Vec<u8>, mime_type: &str) -> Result<()> {
    let Ok(mut tag) = Tag::new().read_from_path(path) else {
        return Ok(());
    };

    let pic_type = if mime_type.contains("png") {
        audiotags::MimeType::Png
    } else {
        audiotags::MimeType::Jpeg
    };

    tag.set_album_cover(audiotags::Picture {
        mime_type: pic_type,
        data: art_bytes.as_slice(),
    });

    tag.write_to_path(path.to_str().unwrap_or(""))?;
    Ok(())
}

/// Sort audio files in a directory by file creation date (oldest = track 1),
/// write the resulting index as the track number tag.
pub fn apply_date_order_track_numbers(dir: &PathBuf) -> Result<Vec<(PathBuf, u32)>> {
    let audio_exts = ["mp3", "m4a", "aac", "flac", "wav", "ogg"];

    let mut files: Vec<(PathBuf, SystemTime)> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file() && p.extension()
                .and_then(|e| e.to_str())
                .map(|e| audio_exts.contains(&e.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .filter_map(|p| {
            let ctime = std::fs::metadata(&p)
                .and_then(|m| m.created())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some((p, ctime))
        })
        .collect();

    // Oldest first → lowest track number
    files.sort_by_key(|(_, t)| *t);

    let mut results = vec![];
    for (i, (path, _)) in files.iter().enumerate() {
        let track_num = (i + 1) as u16;

        match Tag::new().read_from_path(path) {
            Ok(mut tag) => {
                tag.set_track_number(track_num);
                if let Err(e) = tag.write_to_path(path.to_str().unwrap_or("")) {
                    tracing::error!("date_order write failed {}: {}", path.display(), e);
                }
            }
            Err(e) => {
                tracing::warn!("date_order: could not read tags for {}: {}", path.display(), e);
            }
        }

        results.push((path.clone(), track_num as u32));
        tracing::info!("date_order: {:02} → {}", track_num,
            path.file_name().unwrap_or_default().to_string_lossy());
    }

    Ok(results)
}

/// Clean all audio files in a directory recursively
pub fn clean_directory(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let audio_exts = ["mp3", "m4a", "aac", "flac"];
    let mut cleaned = vec![];

    for entry in walkdir::WalkDir::new(dir).min_depth(1) {
        let entry = entry?;
        let path = entry.path().to_path_buf();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if audio_exts.contains(&ext.to_lowercase().as_str()) {
                    clean_file(&path)?;
                    cleaned.push(path);
                }
            }
        }
    }

    Ok(cleaned)
}
