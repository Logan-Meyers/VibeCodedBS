use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Check that ffmpeg is available on PATH
pub fn check_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Convert a single FLAC file to M4A (AAC) at the given bitrate.
/// Output file is placed alongside the input file.
/// Returns the path to the new .m4a file.
pub fn convert_flac_to_m4a(input: &Path, bitrate: &str) -> Result<PathBuf> {
    if !check_ffmpeg() {
        bail!("ffmpeg not found on PATH. Please install ffmpeg to use conversion.");
    }

    let output = input.with_extension("m4a");

    if output.exists() {
        tracing::warn!(
            "convert: output already exists, skipping: {}",
            output.display()
        );
        return Ok(output);
    }

    tracing::info!(
        "convert: {} → {}",
        input.display(),
        output.display()
    );

    let status = Command::new("ffmpeg")
        .args([
            "-i",
            input.to_str().context("invalid input path")?,
            "-c:a",
            "aac",
            "-b:a",
            bitrate,
            "-movflags",
            "+faststart",
            "-vn", // no video stream
            output.to_str().context("invalid output path")?,
        ])
        .status()
        .context("failed to run ffmpeg")?;

    if !status.success() {
        bail!(
            "ffmpeg exited with status {} for {}",
            status,
            input.display()
        );
    }

    Ok(output)
}

/// Convert all FLAC files in a directory (non-recursive by default).
/// Returns list of (original, converted) path pairs.
pub fn convert_directory(
    dir: &Path,
    bitrate: &str,
    recursive: bool,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let walker = if recursive {
        walkdir::WalkDir::new(dir).min_depth(1)
    } else {
        walkdir::WalkDir::new(dir).min_depth(1).max_depth(1)
    };

    let mut results = vec![];

    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.to_lowercase() == "flac" {
                    match convert_flac_to_m4a(path, bitrate) {
                        Ok(out) => results.push((path.to_path_buf(), out)),
                        Err(e) => tracing::error!("convert error {}: {}", path.display(), e),
                    }
                }
            }
        }
    }

    Ok(results)
}
