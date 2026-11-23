//! flux — audio.
//!
//! Ported from `flux` 1.0.0 (generation 1). WAV and ID3v1 are both simple
//! enough to read and write without a dependency, so this crate has none.
//!
//! One scope note: the original's `inspect` claimed to handle several
//! containers but only ever parsed RIFF/WAVE. This one says what it does —
//! WAV headers and ID3v1 tags — and reports anything else as unrecognised
//! rather than guessing.

use savehorse_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MediaMetadata {
    pub file_path: String,
    pub container_format: String,
    pub channels: Option<u16>,
    pub sample_rate: Option<u32>,
    pub bits_per_sample: Option<u16>,
    pub duration_seconds: Option<String>,
    pub byte_rate: Option<u32>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<String>,
    pub extra: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SynthReport {
    pub output_file: String,
    pub frequency_hz: f64,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub total_samples: usize,
    pub file_size_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TagReport {
    pub file: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub replaced_existing: bool,
    pub file_size_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GainReport {
    pub source_file: String,
    pub destination_file: String,
    pub gain_multiplier: f64,
    pub samples_processed: usize,
    pub samples_clipped: usize,
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Trim an ID3v1 field: null or space padded, latin-1.
fn id3_field(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(bytes.len());
    let text = String::from_utf8_lossy(&bytes[..end]).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Read an ID3v1 tag, which lives in the last 128 bytes and begins with "TAG".
fn read_id3v1(data: &[u8]) -> Option<(Option<String>, Option<String>, Option<String>, Option<String>)> {
    if data.len() < 128 {
        return None;
    }
    let tag = &data[data.len() - 128..];
    if &tag[0..3] != b"TAG" {
        return None;
    }

    let title = id3_field(&tag[3..33]);
    let artist = id3_field(&tag[33..63]);
    let album = id3_field(&tag[63..93]);
    let year = id3_field(&tag[93..97]);
    Some((title, artist, album, year))
}

/// Parse a RIFF/WAVE header.
fn read_wav_header(data: &[u8]) -> Result<(u16, u32, u16, u32, u32)> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(Error::Invalid(
            "not a RIFF/WAVE file: the RIFF or WAVE marker is missing".to_string(),
        ));
    }

    // Walk the chunks rather than assuming the fmt chunk sits at a fixed
    // offset: LIST and fact chunks appear before it in real files.
    let mut offset = 12usize;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = read_u32(data, offset + 4) as usize;
        let body = offset + 8;

        if id == b"fmt " && body + 16 <= data.len() {
            let channels = read_u16(data, body + 2);
            let sample_rate = read_u32(data, body + 4);
            let byte_rate = read_u32(data, body + 8);
            let bits = read_u16(data, body + 14);
            return Ok((channels, sample_rate, bits, byte_rate, 0));
        }

        offset = body + size + (size % 2);
    }

    Err(Error::Invalid(
        "not a valid WAV file: no fmt chunk found".to_string(),
    ))
}

fn data_chunk_size(data: &[u8]) -> usize {
    let mut offset = 12usize;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = read_u32(data, offset + 4) as usize;
        if id == b"data" {
            return size.min(data.len().saturating_sub(offset + 8));
        }
        offset = offset + 8 + size + (size % 2);
    }
    0
}

/// Read what a file says about itself: WAV header fields, plus any ID3v1 tag.
pub fn inspect(path: &Path) -> Result<MediaMetadata> {
    let data = fs::read(path)?;
    let mut extra = BTreeMap::new();

    let (container, channels, sample_rate, bits, byte_rate, duration) =
        match read_wav_header(&data) {
            Ok((channels, rate, bits, byte_rate, _)) => {
                let payload = data_chunk_size(&data);
                let seconds = if byte_rate > 0 {
                    payload as f64 / byte_rate as f64
                } else {
                    0.0
                };
                (
                    "RIFF/WAVE".to_string(),
                    Some(channels),
                    Some(rate),
                    Some(bits),
                    Some(byte_rate),
                    Some(format!("{:.3}", seconds)),
                )
            }
            Err(_) => ("unrecognised".to_string(), None, None, None, None, None),
        };

    extra.insert("bytes".to_string(), data.len().to_string());

    let (title, artist, album, year) = match read_id3v1(&data) {
        Some((t, a, al, y)) => {
            extra.insert("id3".to_string(), "ID3v1".to_string());
            (t, a, al, y)
        }
        None => {
            extra.insert("id3".to_string(), "none".to_string());
            (None, None, None, None)
        }
    };

    Ok(MediaMetadata {
        file_path: path.to_string_lossy().to_string(),
        container_format: container,
        channels,
        sample_rate,
        bits_per_sample: bits,
        duration_seconds: duration,
        byte_rate,
        title,
        artist,
        album,
        year,
        extra,
    })
}

/// Write a mono or stereo 16-bit PCM WAV holding a sine tone.
pub fn synthesize(
    out_path: &Path,
    frequency_hz: f64,
    duration_secs: f64,
    sample_rate: u32,
    channels: u16,
    volume: f64,
) -> Result<SynthReport> {
    if sample_rate < 8000 || sample_rate > 192_000 {
        return Err(Error::Invalid(
            "sample rate must be between 8000 and 192000".to_string(),
        ));
    }
    if channels == 0 || channels > 2 {
        return Err(Error::Invalid("channels must be 1 or 2".to_string()));
    }
    if duration_secs <= 0.0 || duration_secs > 600.0 {
        return Err(Error::Invalid(
            "duration must be greater than 0 and at most 600 seconds".to_string(),
        ));
    }

    let samples = (duration_secs * sample_rate as f64).round() as usize;
    let bits_per_sample: u16 = 16;
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * block_align as u32;
    let data_size = (samples * channels as usize * 2) as u32;

    let mut out = Vec::with_capacity(44 + data_size as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());

    let amplitude = (volume.clamp(0.0, 1.0) * i16::MAX as f64 * 0.9) as f64;

    for i in 0..samples {
        let t = i as f64 / sample_rate as f64;
        let value = (amplitude * (2.0 * std::f64::consts::PI * frequency_hz * t).sin()) as i16;
        for _ in 0..channels {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(out_path, &out)?;

    Ok(SynthReport {
        output_file: out_path.to_string_lossy().to_string(),
        frequency_hz,
        duration_seconds: duration_secs,
        sample_rate,
        channels,
        total_samples: samples,
        file_size_bytes: out.len() as u64,
    })
}

fn pad_field(text: &str, width: usize) -> Vec<u8> {
    let mut field = text.as_bytes().iter().copied().take(width).collect::<Vec<u8>>();
    field.resize(width, 0);
    field
}

/// Write or replace an ID3v1 tag: a 128-byte block at the end of the file.
///
/// ID3v1 is latin-1 and truncates hard at 30 characters, which is why the
/// report echoes back exactly what was stored rather than what was asked for.
pub fn write_tag(path: &Path, title: &str, artist: &str, album: &str, year: &str) -> Result<TagReport> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let size = file.metadata()?.len();

    let mut existing = false;
    if size >= 128 {
        file.seek(SeekFrom::End(-128))?;
        let mut magic = [0u8; 3];
        if file.read_exact(&mut magic).is_ok() && &magic == b"TAG" {
            existing = true;
            file.seek(SeekFrom::End(-128))?;
        }
    }
    if !existing {
        file.seek(SeekFrom::End(0))?;
    }

    let mut tag = Vec::with_capacity(128);
    tag.extend_from_slice(b"TAG");
    tag.extend_from_slice(&pad_field(title, 30));
    tag.extend_from_slice(&pad_field(artist, 30));
    tag.extend_from_slice(&pad_field(album, 30));
    tag.extend_from_slice(&pad_field(year, 4));
    tag.extend_from_slice(&pad_field("", 30)); // comment
    tag.push(255); // genre: unset

    file.write_all(&tag)?;
    file.flush()?;

    let new_size = file.metadata()?.len();
    drop(file);

    Ok(TagReport {
        file: path.to_string_lossy().to_string(),
        title: title.chars().take(30).collect(),
        artist: artist.chars().take(30).collect(),
        album: album.chars().take(30).collect(),
        year: year.chars().take(4).collect(),
        replaced_existing: existing,
        file_size_bytes: new_size,
    })
}

/// Scale every 16-bit sample by a multiplier, reporting anything that clipped.
pub fn apply_gain(source: &Path, destination: &Path, multiplier: f64) -> Result<GainReport> {
    let data = fs::read(source)?;
    read_wav_header(&data)?;

    let mut out = data.clone();
    let data_start = find_data_offset(&data)
        .ok_or_else(|| Error::Invalid("no data chunk to scale".to_string()))?;

    // Only whole 16-bit samples are touched; a trailing odd byte is left alone.
    let available = out.len().saturating_sub(data_start) / 2 * 2;
    let mut processed = 0usize;
    let mut clipped = 0usize;

    let mut offset = data_start;
    while offset + 2 <= data_start + available {
        let sample = i16::from_le_bytes([out[offset], out[offset + 1]]);
        let scaled = (sample as f64 * multiplier).round();

        let value = if scaled > i16::MAX as f64 {
            clipped += 1;
            i16::MAX
        } else if scaled < i16::MIN as f64 {
            clipped += 1;
            i16::MIN
        } else {
            scaled as i16
        };

        out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        offset += 2;
        processed += 1;
    }

    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(destination, &out)?;

    Ok(GainReport {
        source_file: source.to_string_lossy().to_string(),
        destination_file: destination.to_string_lossy().to_string(),
        gain_multiplier: multiplier,
        samples_processed: processed,
        samples_clipped: clipped,
    })
}

fn find_data_offset(data: &[u8]) -> Option<usize> {
    let mut offset = 12usize;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = read_u32(data, offset + 4) as usize;
        if id == b"data" {
            return Some(offset + 8);
        }
        offset = offset + 8 + size + (size % 2);
    }
    None
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
        p.push(format!("savehorse-flux-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn tone(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        synthesize(&path, 440.0, 0.05, 8000, 1, 0.5).unwrap();
        path
    }

    #[test]
    fn synthesises_a_readable_wav() {
        let dir = scratch("synth");
        let path = dir.join("tone.wav");

        let report = synthesize(&path, 440.0, 0.1, 8000, 1, 0.5).unwrap();
        assert_eq!(report.total_samples, 800);
        assert_eq!(report.file_size_bytes, 44 + 800 * 2);

        let info = inspect(&path).unwrap();
        assert_eq!(info.container_format, "RIFF/WAVE");
        assert_eq!(info.sample_rate, Some(8000));
        assert_eq!(info.channels, Some(1));
        assert_eq!(info.bits_per_sample, Some(16));
        assert_eq!(info.duration_seconds.as_deref(), Some("0.100"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn synth_rejects_impossible_settings() {
        let dir = scratch("synth-bad");
        let path = dir.join("x.wav");
        assert!(synthesize(&path, 440.0, 0.0, 8000, 1, 0.5).is_err(), "zero duration");
        assert!(synthesize(&path, 440.0, 0.1, 100, 1, 0.5).is_err(), "sample rate too low");
        assert!(synthesize(&path, 440.0, 0.1, 8000, 3, 0.5).is_err(), "three channels");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_and_reads_an_id3v1_tag() {
        let dir = scratch("tag");
        let path = tone(&dir, "tone.wav");
        let before = fs::metadata(&path).unwrap().len();

        let report = write_tag(&path, "Test Tone", "savehorse", "Album", "2026").unwrap();
        assert!(!report.replaced_existing);
        assert_eq!(report.file_size_bytes, before + 128);

        let info = inspect(&path).unwrap();
        assert_eq!(info.title.as_deref(), Some("Test Tone"));
        assert_eq!(info.artist.as_deref(), Some("savehorse"));
        assert_eq!(info.year.as_deref(), Some("2026"));

        // Tagging again replaces the tag rather than stacking a second one.
        let again = write_tag(&path, "Second", "savehorse", "Album", "2026").unwrap();
        assert!(again.replaced_existing);
        assert_eq!(again.file_size_bytes, before + 128);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_long_title_is_truncated_to_thirty() {
        let dir = scratch("tag-long");
        let path = tone(&dir, "tone.wav");
        let long = "a".repeat(60);

        let report = write_tag(&path, &long, "", "", "").unwrap();
        assert_eq!(report.title.chars().count(), 30);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn gain_doubles_the_samples_and_reports_clipping() {
        let dir = scratch("gain");
        let path = tone(&dir, "tone.wav");
        let out = dir.join("loud.wav");

        let report = apply_gain(&path, &out, 2.0).unwrap();
        assert_eq!(report.samples_processed, 400);
        assert_eq!(report.samples_clipped, 0, "0.5 volume at 0.9 peak stays in range");
        assert_eq!(fs::metadata(&out).unwrap().len(), fs::metadata(&path).unwrap().len());

        let hot = apply_gain(&path, &dir.join("hot.wav"), 10.0).unwrap();
        assert!(hot.samples_clipped > 0, "a 10x gain must clip");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn inspect_refuses_to_guess_at_a_non_wav() {
        let dir = scratch("not-wav");
        let path = dir.join("notes.txt");
        fs::write(&path, b"this is not audio").unwrap();

        let info = inspect(&path).unwrap();
        assert_eq!(info.container_format, "unrecognised");
        assert_eq!(info.sample_rate, None);
        let _ = fs::remove_dir_all(&dir);
    }
}
