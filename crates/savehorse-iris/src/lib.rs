//! iris — images.
//!
//! Ported from `iris` 1.0.0 (generation 2), which decoded with the `image`
//! crate. One defect is fixed: the original batch processor ignored every save
//! error with `let _ = img.save(...)` and still counted the file as processed,
//! so a run could report success while writing nothing. Failures are now
//! collected and returned.

use image::ColorType;
use savehorse_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Extensions the batch operation will pick up.
const SOURCE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "ico"];

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ImageInfo {
    pub file: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub aspect_ratio: String,
    pub aspect_ratio_float: f64,
    pub color_type: String,
    pub byte_size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConvertResult {
    pub source_file: String,
    pub output_file: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub output_byte_size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ThumbResult {
    pub source_file: String,
    pub output_file: String,
    pub original_dimensions: String,
    pub thumbnail_dimensions: String,
    pub output_byte_size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TransformResult {
    pub source_file: String,
    pub output_file: String,
    pub rotation: Option<u32>,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub new_dimensions: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChannelStats {
    pub file: String,
    pub width: u32,
    pub height: u32,
    pub total_pixels: u64,
    pub mean_red: f64,
    pub mean_green: f64,
    pub mean_blue: f64,
    pub mean_alpha: f64,
    pub mean_luminance: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BatchResult {
    pub source_dir: String,
    pub output_dir: String,
    pub target_format: String,
    pub thumbnail_size: Option<u32>,
    pub processed: Vec<String>,
    pub failed: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Reduce width and height to the smallest whole ratio, and give the float.
pub fn format_aspect_ratio(w: u32, h: u32) -> (String, f64) {
    if w == 0 || h == 0 {
        return ("0:0".to_string(), 0.0);
    }
    let d = gcd(w, h);
    let text = format!("{}:{}", w / d, h / d);
    let float = (w as f64 / h as f64 * 100.0).round() / 100.0;
    (text, float)
}

fn color_type_name(c: ColorType) -> &'static str {
    match c {
        ColorType::L8 => "8-bit grayscale",
        ColorType::La8 => "8-bit grayscale with alpha",
        ColorType::Rgb8 => "24-bit RGB",
        ColorType::Rgba8 => "32-bit RGBA",
        ColorType::L16 => "16-bit grayscale",
        ColorType::La16 => "16-bit grayscale with alpha",
        ColorType::Rgb16 => "48-bit RGB",
        ColorType::Rgba16 => "64-bit RGBA",
        ColorType::Rgb32F => "96-bit float RGB",
        ColorType::Rgba32F => "128-bit float RGBA",
        _ => "custom",
    }
}

fn open(path: &Path) -> Result<image::DynamicImage> {
    image::open(path).map_err(|e| Error::Invalid(format!("{}: {}", path.display(), e)))
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn extension_upper(path: &Path, fallback: &str) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or(fallback)
        .to_uppercase()
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// Dimensions, format, colour depth and aspect ratio.
pub fn inspect_image(path: &Path) -> Result<ImageInfo> {
    let meta = fs::metadata(path)?;
    let img = open(path)?;
    let (w, h) = (img.width(), img.height());
    let (ratio_text, ratio_float) = format_aspect_ratio(w, h);

    Ok(ImageInfo {
        file: path.to_string_lossy().to_string(),
        format: extension_upper(path, "unknown"),
        width: w,
        height: h,
        aspect_ratio: ratio_text,
        aspect_ratio_float: ratio_float,
        color_type: color_type_name(img.color()).to_string(),
        byte_size: meta.len(),
    })
}

/// Decode and re-encode in the format the output extension implies.
pub fn convert_image(source: &Path, out_path: &Path) -> Result<ConvertResult> {
    let img = open(source)?;
    ensure_parent(out_path)?;
    img.save(out_path)
        .map_err(|e| Error::Invalid(format!("{}: {}", out_path.display(), e)))?;

    let (w, h) = (img.width(), img.height());
    let size = fs::metadata(out_path)?.len();

    Ok(ConvertResult {
        source_file: source.to_string_lossy().to_string(),
        output_file: out_path.to_string_lossy().to_string(),
        format: extension_upper(out_path, "png"),
        width: w,
        height: h,
        output_byte_size: size,
    })
}

/// Downscale to fit inside the given box, keeping the aspect ratio.
pub fn thumb_image(source: &Path, out_path: &Path, width: u32, height: u32) -> Result<ThumbResult> {
    if width == 0 || height == 0 {
        return Err(Error::Invalid("width and height must be greater than zero".to_string()));
    }

    let img = open(source)?;
    let (orig_w, orig_h) = (img.width(), img.height());
    let thumb = img.thumbnail(width, height);
    let (new_w, new_h) = (thumb.width(), thumb.height());

    ensure_parent(out_path)?;
    thumb
        .save(out_path)
        .map_err(|e| Error::Invalid(format!("{}: {}", out_path.display(), e)))?;
    let size = fs::metadata(out_path)?.len();

    Ok(ThumbResult {
        source_file: source.to_string_lossy().to_string(),
        output_file: out_path.to_string_lossy().to_string(),
        original_dimensions: format!("{}x{}", orig_w, orig_h),
        thumbnail_dimensions: format!("{}x{}", new_w, new_h),
        output_byte_size: size,
    })
}

/// Rotate in right angles, flip on either axis, or both.
pub fn transform_image(
    source: &Path,
    out_path: &Path,
    rotate: Option<u32>,
    flip_h: bool,
    flip_v: bool,
) -> Result<TransformResult> {
    let mut img = open(source)?;

    if let Some(deg) = rotate {
        img = match deg {
            90 => img.rotate90(),
            180 => img.rotate180(),
            270 => img.rotate270(),
            _ => {
                return Err(Error::Invalid(
                    "rotation must be 90, 180 or 270 degrees".to_string(),
                ))
            }
        };
    }
    if flip_h {
        img = img.fliph();
    }
    if flip_v {
        img = img.flipv();
    }

    ensure_parent(out_path)?;
    img.save(out_path)
        .map_err(|e| Error::Invalid(format!("{}: {}", out_path.display(), e)))?;

    Ok(TransformResult {
        source_file: source.to_string_lossy().to_string(),
        output_file: out_path.to_string_lossy().to_string(),
        rotation: rotate,
        flip_horizontal: flip_h,
        flip_vertical: flip_v,
        new_dimensions: format!("{}x{}", img.width(), img.height()),
    })
}

/// Mean channel values and BT.601 luminance, over every pixel.
pub fn stats_image(path: &Path) -> Result<ChannelStats> {
    let img = open(path)?;
    let (w, h) = (img.width(), img.height());
    let total_pixels = (w as u64) * (h as u64);

    let mut sum_r = 0u64;
    let mut sum_g = 0u64;
    let mut sum_b = 0u64;
    let mut sum_a = 0u64;
    let mut sum_luminance = 0.0f64;

    let rgba = img.to_rgba8();
    for pixel in rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        let (r, g, b, a) = (r as u64, g as u64, b as u64, a as u64);
        sum_r += r;
        sum_g += g;
        sum_b += b;
        sum_a += a;
        sum_luminance += (0.2126 * r as f64) + (0.7152 * g as f64) + (0.0722 * b as f64);
    }

    let denom = if total_pixels > 0 {
        total_pixels as f64
    } else {
        1.0
    };

    Ok(ChannelStats {
        file: path.to_string_lossy().to_string(),
        width: w,
        height: h,
        total_pixels,
        mean_red: (sum_r as f64 / denom * 100.0).round() / 100.0,
        mean_green: (sum_g as f64 / denom * 100.0).round() / 100.0,
        mean_blue: (sum_b as f64 / denom * 100.0).round() / 100.0,
        mean_alpha: (sum_a as f64 / denom * 100.0).round() / 100.0,
        mean_luminance: (sum_luminance / denom * 100.0).round() / 100.0,
    })
}

/// Convert every image in a directory, optionally shrinking each one.
///
/// Failures are reported rather than swallowed: a file that cannot be read or
/// written appears in `failed` and is not counted as processed.
pub fn batch_process(
    in_dir: &Path,
    out_dir: &Path,
    format: &str,
    thumb_size: Option<u32>,
) -> Result<BatchResult> {
    let target = format.trim().trim_start_matches('.').to_lowercase();
    if target.is_empty() {
        return Err(Error::Invalid("a target format is required, e.g. png".to_string()));
    }
    fs::create_dir_all(out_dir)?;

    let mut processed = Vec::new();
    let mut failed = Vec::new();

    let mut entries: Vec<_> = fs::read_dir(in_dir)?.flatten().map(|e| e.path()).collect();
    entries.sort();

    for path in entries {
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !SOURCE_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        let img = match image::open(&path) {
            Ok(i) => i,
            Err(e) => {
                failed.push(format!("{}: {}", path.display(), e));
                continue;
            }
        };

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();
        let dest = out_dir.join(format!("{}.{}", stem, target));

        let saved = match thumb_size {
            Some(size) => img.thumbnail(size, size).save(&dest),
            None => img.save(&dest),
        };

        match saved {
            Ok(()) => processed.push(dest.to_string_lossy().to_string()),
            Err(e) => failed.push(format!("{}: {}", dest.display(), e)),
        }
    }

    Ok(BatchResult {
        source_dir: in_dir.to_string_lossy().to_string(),
        output_dir: out_dir.to_string_lossy().to_string(),
        target_format: target,
        thumbnail_size: thumb_size,
        processed,
        failed,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-iris-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// A 4x2 image: left half red, right half blue.
    fn make_sample(dir: &Path) -> PathBuf {
        let mut img = image::RgbaImage::new(4, 2);
        for (x, _y, pixel) in img.enumerate_pixels_mut() {
            *pixel = if x < 2 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            };
        }
        let path = dir.join("sample.png");
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn aspect_ratio_reduces_to_whole_numbers() {
        assert_eq!(format_aspect_ratio(1920, 1080).0, "16:9");
        assert_eq!(format_aspect_ratio(4, 2).0, "2:1");
        assert_eq!(format_aspect_ratio(0, 0).0, "0:0");
        assert_eq!(format_aspect_ratio(4, 2).1, 2.0);
    }

    #[test]
    fn inspects_an_image() {
        let dir = scratch("info");
        let path = make_sample(&dir);
        let info = inspect_image(&path).unwrap();

        assert_eq!(info.width, 4);
        assert_eq!(info.height, 2);
        assert_eq!(info.format, "PNG");
        assert_eq!(info.aspect_ratio, "2:1");
        assert!(info.byte_size > 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn converts_between_formats() {
        let dir = scratch("convert");
        let src = make_sample(&dir);
        let out = dir.join("out.bmp");

        let result = convert_image(&src, &out).unwrap();
        assert_eq!(result.format, "BMP");
        assert_eq!(result.width, 4);
        assert!(out.exists());

        // And the output is readable again.
        let back = inspect_image(&out).unwrap();
        assert_eq!(back.width, 4);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn thumbnails_preserve_aspect_ratio() {
        let dir = scratch("thumb");
        let src = make_sample(&dir);
        let out = dir.join("thumb.png");

        let result = thumb_image(&src, &out, 2, 2).unwrap();
        assert_eq!(result.original_dimensions, "4x2");
        // Fitting a 4x2 into a 2x2 box gives 2x1.
        assert_eq!(result.thumbnail_dimensions, "2x1");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn thumbnail_rejects_a_zero_box() {
        let dir = scratch("thumb-zero");
        let src = make_sample(&dir);
        assert!(thumb_image(&src, &dir.join("t.png"), 0, 10).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_swaps_dimensions() {
        let dir = scratch("rotate");
        let src = make_sample(&dir);
        let out = dir.join("rot.png");

        let result = transform_image(&src, &out, Some(90), false, false).unwrap();
        assert_eq!(result.new_dimensions, "2x4");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn transform_rejects_a_bad_angle() {
        let dir = scratch("rotate-bad");
        let src = make_sample(&dir);
        let err = transform_image(&src, &dir.join("x.png"), Some(45), false, false).unwrap_err();
        assert!(err.to_string().contains("90, 180 or 270"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stats_average_the_channels() {
        let dir = scratch("stats");
        let src = make_sample(&dir);
        let stats = stats_image(&src).unwrap();

        assert_eq!(stats.total_pixels, 8);
        assert_eq!(stats.mean_red, 127.5, "half the pixels are pure red");
        assert_eq!(stats.mean_blue, 127.5);
        assert_eq!(stats.mean_green, 0.0);
        assert_eq!(stats.mean_alpha, 255.0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_reports_what_it_did_and_what_it_could_not() {
        let dir = scratch("batch");
        let src = dir.join("in");
        fs::create_dir_all(&src).unwrap();
        make_sample(&src);

        // A file with an image extension that is not an image at all.
        fs::write(src.join("broken.png"), b"not an image").unwrap();

        let out = dir.join("out");
        let result = batch_process(&src, &out, "bmp", None).unwrap();

        assert_eq!(result.processed.len(), 1);
        assert_eq!(result.failed.len(), 1, "the broken file must be reported, not ignored");
        assert!(result.failed[0].contains("broken.png"));
        assert!(out.join("sample.bmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn batch_can_shrink_everything() {
        let dir = scratch("batch-thumb");
        let src = dir.join("in");
        fs::create_dir_all(&src).unwrap();
        make_sample(&src);

        let out = dir.join("out");
        let result = batch_process(&src, &out, "png", Some(2)).unwrap();
        assert_eq!(result.processed.len(), 1);

        let produced = inspect_image(&out.join("sample.png")).unwrap();
        assert_eq!(produced.width, 2);
        let _ = fs::remove_dir_all(&dir);
    }
}
