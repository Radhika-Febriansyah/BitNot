//! Artwork cache: one folder of cover images shared by all profiles.
//!
//! Each image is stored once under a hash of its original bytes, as
//!   `<hash>.<ext>`        the display image (longest side at most 1200 px), or
//!   `<hash>.anim.<ext>`   an animated original, kept byte-for-byte for later
//!                         animated-artwork support (not played back yet), and
//!   `<hash>.thumb.jpg`    a 320 px static thumbnail used by lists and grids.
//!
//! Music files and the user's own image files are only ever read.

use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::{Duration, SystemTime};

use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};
use sha2::{Digest, Sha256};

const FULL_MAX: u32 = 1200;
const THUMB_MAX: u32 = 320;
const THUMB_SUFFIX: &str = ".thumb.jpg";
pub const MAX_SOURCE_BYTES: u64 = 20 * 1024 * 1024;

/// Name of the thumbnail belonging to a cached image name.
pub fn thumb_name(name: &str) -> String {
    let hash = name.split('.').next().unwrap_or(name);
    format!("{hash}{THUMB_SUFFIX}")
}

fn is_animated(format: ImageFormat, bytes: &[u8]) -> bool {
    match format {
        // A GIF with more than one image descriptor after the first frame.
        ImageFormat::Gif => bytes.windows(3).filter(|w| w == b"\x00\x21\xF9").count() > 1,
        ImageFormat::WebP => {
            bytes.len() > 16 && bytes[12..].windows(4).take(64).any(|w| w == b"ANIM")
        }
        _ => false,
    }
}

fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> Option<()> {
    let temp = dir.join(format!("{name}.tmp"));
    fs::write(&temp, bytes).ok()?;
    fs::rename(&temp, dir.join(name)).ok()
}

fn encode_jpeg(image: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    // JPEG has no transparency; covers are flattened onto black.
    DynamicImage::ImageRgb8(image.to_rgb8())
        .write_to(&mut out, ImageFormat::Jpeg)
        .ok()?;
    Some(out.into_inner())
}

fn write_thumb(dir: &Path, name: &str, image: &DynamicImage) -> Option<()> {
    let thumb = image.resize(THUMB_MAX, THUMB_MAX, FilterType::Triangle);
    write_atomic(dir, &thumb_name(name), &encode_jpeg(&thumb)?)
}

/// Stores an image in the cache and returns its cache name.
/// Returns `None` when the bytes are not a readable image.
pub fn store(cache_dir: &Path, bytes: &[u8]) -> Option<String> {
    let format = image::guess_format(bytes).ok()?;
    let ext = match format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Png => "png",
        ImageFormat::Gif => "gif",
        ImageFormat::WebP => "webp",
        ImageFormat::Bmp => "bmp",
        _ => return None,
    };
    let digest = Sha256::digest(bytes);
    let hash: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    let animated = is_animated(format, bytes);

    // Already cached under any of the names this image could have.
    for candidate in [
        format!("{hash}.{ext}"),
        format!("{hash}.anim.{ext}"),
        format!("{hash}.jpg"),
        format!("{hash}.png"),
    ] {
        if cache_dir.join(&candidate).exists() {
            if !cache_dir.join(thumb_name(&candidate)).exists() {
                let image = image::load_from_memory(bytes).ok()?;
                write_thumb(cache_dir, &candidate, &image)?;
            }
            return Some(candidate);
        }
    }

    let image = image::load_from_memory(bytes).ok()?;
    let name = if animated {
        let name = format!("{hash}.anim.{ext}");
        write_atomic(cache_dir, &name, bytes)?;
        name
    } else if image.width() > FULL_MAX || image.height() > FULL_MAX {
        let resized = image.resize(FULL_MAX, FULL_MAX, FilterType::Lanczos3);
        if image.color().has_alpha() {
            let mut out = Cursor::new(Vec::new());
            resized.write_to(&mut out, ImageFormat::Png).ok()?;
            let name = format!("{hash}.png");
            write_atomic(cache_dir, &name, &out.into_inner())?;
            name
        } else {
            let name = format!("{hash}.jpg");
            write_atomic(cache_dir, &name, &encode_jpeg(&resized)?)?;
            name
        }
    } else {
        let name = format!("{hash}.{ext}");
        write_atomic(cache_dir, &name, bytes)?;
        name
    };
    write_thumb(cache_dir, &name, &image)?;
    Some(name)
}

/// Imports an image file the user chose (the file itself is only read).
pub fn import_file(cache_dir: &Path, path: &Path) -> Result<String, String> {
    let size = fs::metadata(path)
        .map_err(|_| "The image could not be opened.".to_string())?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err("The image is larger than 20 MB.".to_string());
    }
    let bytes = fs::read(path).map_err(|_| "The image could not be opened.".to_string())?;
    store(cache_dir, &bytes)
        .ok_or_else(|| "That file is not a JPEG, PNG, WebP, GIF, or BMP image.".to_string())
}

/// Creates missing thumbnails for images cached by earlier versions.
pub fn backfill_thumbs(cache_dir: &Path) -> usize {
    let mut made = 0;
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(THUMB_SUFFIX) || name.ends_with(".tmp") {
            continue;
        }
        if cache_dir.join(thumb_name(&name)).exists() {
            continue;
        }
        if let Ok(image) = image::open(entry.path()) {
            if write_thumb(cache_dir, &name, &image).is_some() {
                made += 1;
            }
        }
    }
    made
}

/// Deletes cached images that nothing refers to any more. Files newer than
/// `min_age` are kept, so an image stored by a scan in progress is not removed.
/// Returns the number of files deleted.
pub fn cleanup(cache_dir: &Path, referenced: &HashSet<String>, min_age: Duration) -> usize {
    let keep: HashSet<String> = referenced
        .iter()
        .flat_map(|name| [name.clone(), thumb_name(name)])
        .collect();
    let now = SystemTime::now();
    let mut removed = 0;
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if keep.contains(&name) || !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= min_age);
        if old_enough && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    pub fn png(width: u32, height: u32, shade: u8) -> Vec<u8> {
        let image = RgbImage::from_pixel(width, height, Rgb([shade, 80, 160]));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn size_of(path: &Path) -> (u32, u32) {
        let image = image::open(path).unwrap();
        (image.width(), image.height())
    }

    #[test]
    fn small_images_are_kept_as_is_with_a_thumbnail() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = png(400, 400, 10);
        let name = store(tmp.path(), &bytes).unwrap();
        assert!(name.ends_with(".png"));
        assert_eq!(fs::read(tmp.path().join(&name)).unwrap(), bytes);
        assert_eq!(size_of(&tmp.path().join(thumb_name(&name))), (320, 320));
    }

    #[test]
    fn large_images_are_downscaled_keeping_proportions() {
        let tmp = tempfile::tempdir().unwrap();
        let name = store(tmp.path(), &png(3000, 1500, 20)).unwrap();
        assert!(name.ends_with(".jpg"));
        assert_eq!(size_of(&tmp.path().join(&name)), (1200, 600));
        assert_eq!(size_of(&tmp.path().join(thumb_name(&name))), (320, 160));
    }

    #[test]
    fn transparency_survives_downscaling() {
        let tmp = tempfile::tempdir().unwrap();
        let image = RgbaImage::from_pixel(1600, 1600, Rgba([1, 2, 3, 120]));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Png).unwrap();
        let name = store(tmp.path(), &out.into_inner()).unwrap();
        assert!(name.ends_with(".png"));
        assert!(image::open(tmp.path().join(&name))
            .unwrap()
            .color()
            .has_alpha());
    }

    #[test]
    fn the_same_image_is_stored_once() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = png(3000, 3000, 30);
        let first = store(tmp.path(), &bytes).unwrap();
        let second = store(tmp.path(), &bytes).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fs::read_dir(tmp.path()).unwrap().count(),
            2,
            "image and thumbnail"
        );
    }

    #[test]
    fn non_images_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(store(tmp.path(), b"definitely not an image"), None);
        // Looks like a PNG by its first bytes but cannot be decoded.
        assert_eq!(store(tmp.path(), b"\x89PNG\r\n\x1a\nbroken"), None);
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn import_reads_the_users_file_without_changing_it() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        let source = tmp.path().join("my cover.png");
        let bytes = png(500, 500, 40);
        fs::write(&source, &bytes).unwrap();
        let name = import_file(&cache, &source).unwrap();
        assert!(cache.join(&name).exists());
        assert_eq!(fs::read(&source).unwrap(), bytes);

        fs::write(tmp.path().join("notes.txt"), "hello").unwrap();
        assert!(import_file(&cache, &tmp.path().join("notes.txt")).is_err());
        assert!(import_file(&cache, &tmp.path().join("missing.png")).is_err());
    }

    #[test]
    fn cleanup_removes_only_unreferenced_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let kept = store(tmp.path(), &png(100, 100, 1)).unwrap();
        let dropped = store(tmp.path(), &png(100, 100, 2)).unwrap();
        let referenced: HashSet<String> = [kept.clone()].into();

        // Too new to remove yet.
        assert_eq!(
            cleanup(tmp.path(), &referenced, Duration::from_secs(3600)),
            0
        );
        assert_eq!(cleanup(tmp.path(), &referenced, Duration::ZERO), 2);
        assert!(tmp.path().join(&kept).exists());
        assert!(tmp.path().join(thumb_name(&kept)).exists());
        assert!(!tmp.path().join(&dropped).exists());
    }

    #[test]
    fn missing_thumbnails_are_recreated() {
        let tmp = tempfile::tempdir().unwrap();
        let name = store(tmp.path(), &png(600, 600, 5)).unwrap();
        fs::remove_file(tmp.path().join(thumb_name(&name))).unwrap();
        assert_eq!(backfill_thumbs(tmp.path()), 1);
        assert!(tmp.path().join(thumb_name(&name)).exists());
        assert_eq!(backfill_thumbs(tmp.path()), 0);
    }
}
