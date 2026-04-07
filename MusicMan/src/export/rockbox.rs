use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn export_to_rockbox(
    working_dir: &Path,
    ipod_root: &Path,
) -> Result<ExportReport> {
    let music_root = ipod_root.join("Music");
    std::fs::create_dir_all(&music_root)?;

    let mut report = ExportReport::default();

    for entry in walkdir::WalkDir::new(working_dir).min_depth(1) {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() { continue; }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

        if is_audio_ext(&ext) {
            match export_audio_file(path, working_dir, &music_root) {
                Ok(dest) => { report.exported.push(dest); }
                Err(e) => { report.errors.push(format!("{}: {}", path.display(), e)); }
            }
        }

        if is_image_ext(&ext) {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(stem, "cover" | "folder" | "artwork") {
                match export_cover_art_as_bmp(path, working_dir, &music_root) {
                    Ok(dest) => { report.art_exported.push(dest); }
                    Err(e) => { tracing::warn!("art error {}: {}", path.display(), e); }
                }
            }
        }
    }

    Ok(report)
}

fn export_audio_file(src: &Path, _working_dir: &Path, music_root: &Path) -> Result<PathBuf> {
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let tags = read_tags_for_export(src);

    let artist = sanitize(&tags.artist.unwrap_or_else(|| "Unknown Artist".into()));
    let album  = sanitize(&tags.album.unwrap_or_else(|| "Unknown Album".into()));
    let title  = sanitize(&tags.title.unwrap_or_else(|| {
        src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Unknown".into())
    }));
    let track_number = tags.track_number.map(|n| format!("{:02}", n)).unwrap_or_else(|| "00".into());

    let dest = music_root.join(&artist).join(&album).join(format!("{} - {}.{}", track_number, title, ext));
    if let Some(parent) = dest.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::copy(src, &dest).with_context(|| format!("copy {} -> {}", src.display(), dest.display()))?;
    Ok(dest)
}

fn export_cover_art_as_bmp(src: &Path, working_dir: &Path, music_root: &Path) -> Result<PathBuf> {
    let rel = src.parent()
        .and_then(|p| p.strip_prefix(working_dir).ok())
        .unwrap_or(Path::new(""));
    let dest_dir = music_root.join(rel);
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join("cover.bmp");
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext == "bmp" {
        std::fs::copy(src, &dest)?;
    } else {
        // Full JPEG/PNG->BMP requires image crate (rustc >= 1.80).
        // On your local machine: uncomment image dep and replace this with:
        //   let img = image::open(src)?;
        //   let img = img.resize(300, 300, image::imageops::FilterType::Lanczos3);
        //   img.save_with_format(&dest, image::ImageFormat::Bmp)?;
        std::fs::copy(src, &dest)?;
        tracing::warn!("cover copied without BMP conversion (needs rustc >= 1.80): {}", src.display());
    }
    Ok(dest)
}

fn is_audio_ext(ext: &str) -> bool {
    matches!(ext, "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg")
}

fn is_image_ext(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "bmp" | "webp")
}

pub fn sanitize(name: &str) -> String {
    name.chars().map(|c| match c {
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
        c => c,
    }).collect::<String>().trim().to_string()
}

struct ExportTags {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    track_number: Option<u32>,
}

fn read_tags_for_export(path: &Path) -> ExportTags {
    use audiotags::Tag;
    if let Ok(tag) = Tag::new().read_from_path(path) {
        return ExportTags {
            title: tag.title().map(String::from),
            artist: tag.artist().map(String::from),
            album: tag.album().map(|a| a.title.to_string()),
            track_number: tag.track_number().map(|n| n as u32),
        };
    }
    ExportTags { title: None, artist: None, album: None, track_number: None }
}

#[derive(Debug, Default)]
pub struct ExportReport {
    pub exported: Vec<PathBuf>,
    pub art_exported: Vec<PathBuf>,
    pub errors: Vec<String>,
}

impl ExportReport {
    pub fn summary(&self) -> String {
        format!("Exported: {} tracks, {} cover art files, {} errors",
            self.exported.len(), self.art_exported.len(), self.errors.len())
    }
}
