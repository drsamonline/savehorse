//! aegis — vault and security.
//!
//! Two applications live here:
//!
//! * `aegis` 1.0.0 (generation 1) — shredding, threat heuristics and a
//!   binary-hardening audit.
//! * `vault` 1.0.0 (generation 2) — authenticated encryption.
//!
//! **The generation 1 cryptography is gone.** `aegis` derived keys from a
//! hand-written linear congruential generator seeded with the file size, and
//! used a home-made CRC-32 as its integrity check. That is not a random number
//! generator and it is not an authenticator. The encryption here is `vault`'s:
//! AES-256-GCM with PBKDF2-HMAC-SHA256 over 100,000 rounds and a random salt,
//! from the OS entropy source.
//!
//! The container keeps vault's `VAULT\x01` header byte for byte, so files
//! encrypted by the old `vault` still open. Containers written by the old
//! `aegis` (magic `AEGISENC`) do **not** open here: they were encrypted with
//! the broken generator, and the honest migration is to decrypt them once with
//! the old binary and re-encrypt with this one.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::{thread_rng, RngCore};
use savehorse_core::{crc32, Error, Result};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Kept from vault so existing containers remain readable.
const MAGIC: &[u8; 6] = b"VAULT\x01";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const PBKDF2_ROUNDS: u32 = 100_000;
const CHUNK: usize = 65536;

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CryptoReport {
    pub operation: String,
    pub input_file: String,
    pub output_file: String,
    pub input_size_bytes: u64,
    pub output_size_bytes: u64,
    pub cipher: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ShredReport {
    pub original_path: String,
    pub original_size_bytes: u64,
    pub passes_completed: usize,
    pub method: String,
    pub file_unlinked: bool,
    pub dry_run: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ThreatItem {
    pub file_path: String,
    pub threat_name: String,
    pub threat_type: String,
    pub severity: String,
    pub detail: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    pub target_path: String,
    pub files_scanned: usize,
    pub threats_found: usize,
    pub clean: bool,
    /// True when a signature database was supplied. Heuristics always run.
    pub signatures_loaded: usize,
    pub threats: Vec<ThreatItem>,
}

/// One entry in a signature database: a name and the CRC-32 of the file.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub crc32: u32,
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

fn derive_key(passphrase: Option<&str>, keyfile: Option<&Path>, salt: &[u8]) -> Result<[u8; 32]> {
    let mut secret: Vec<u8> = Vec::new();

    if let Some(p) = passphrase {
        secret.extend_from_slice(p.as_bytes());
    }
    if let Some(kf) = keyfile {
        let bytes = fs::read(kf)
            .map_err(|e| Error::Invalid(format!("cannot read keyfile '{}': {}", kf.display(), e)))?;
        secret.extend_from_slice(&bytes);
    }

    if secret.is_empty() {
        return Err(Error::Invalid(
            "a passphrase or a keyfile is required".to_string(),
        ));
    }

    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(&secret, salt, PBKDF2_ROUNDS, &mut key);

    // Do not leave the passphrase material lying around.
    secret.fill(0);
    Ok(key)
}

// ---------------------------------------------------------------------------
// Encryption
// ---------------------------------------------------------------------------

/// Encrypt a file into a container: magic, salt, nonce, ciphertext.
pub fn encrypt(
    input: &Path,
    output: Option<&Path>,
    passphrase: Option<&str>,
    keyfile: Option<&Path>,
) -> Result<CryptoReport> {
    let plaintext = fs::read(input)?;
    let input_size = plaintext.len() as u64;

    let mut rng = thread_rng();
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill_bytes(&mut salt);
    rng.fill_bytes(&mut nonce_bytes);

    let mut key = derive_key(passphrase, keyfile, &salt)?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|_| Error::Invalid("encryption failed".to_string()))?;

    key.fill(0);

    let destination = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let name = input
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());
            input.with_file_name(format!("{}.vault", name))
        }
    };

    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut out = File::create(&destination)?;
    out.write_all(MAGIC)?;
    out.write_all(&salt)?;
    out.write_all(&nonce_bytes)?;
    out.write_all(&ciphertext)?;
    out.flush()?;

    Ok(CryptoReport {
        operation: "encrypt".to_string(),
        input_file: input.to_string_lossy().to_string(),
        output_file: destination.to_string_lossy().to_string(),
        input_size_bytes: input_size,
        output_size_bytes: fs::metadata(&destination)?.len(),
        cipher: "AES-256-GCM".to_string(),
    })
}

/// Decrypt a container produced by [`encrypt`].
pub fn decrypt(
    container: &Path,
    output: Option<&Path>,
    passphrase: Option<&str>,
    keyfile: Option<&Path>,
) -> Result<CryptoReport> {
    let mut file = File::open(container)?;
    let file_len = file.metadata()?.len();

    let header_len = (MAGIC.len() + SALT_LEN + NONCE_LEN) as u64;
    if file_len < header_len {
        return Err(Error::Invalid(
            "not a container: the file is shorter than its own header".to_string(),
        ));
    }

    let mut magic = [0u8; 6];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(Error::Invalid(
            "not a container this build can read: expected a VAULT header".to_string(),
        ));
    }

    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    file.read_exact(&mut salt)?;
    file.read_exact(&mut nonce_bytes)?;

    let mut ciphertext = Vec::new();
    file.read_to_end(&mut ciphertext)?;

    let mut key = derive_key(passphrase, keyfile, &salt)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    // A wrong passphrase or a tampered byte both land here, by design: GCM
    // cannot tell you which, and should not.
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| {
            Error::Invalid(
                "could not decrypt: wrong passphrase, wrong keyfile, or the file was altered"
                    .to_string(),
            )
        })?;

    key.fill(0);

    let destination = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let name = container
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());
            let stripped = name.strip_suffix(".vault").unwrap_or(&name).to_string();
            container.with_file_name(stripped)
        }
    };

    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&destination, &plaintext)?;

    Ok(CryptoReport {
        operation: "decrypt".to_string(),
        input_file: container.to_string_lossy().to_string(),
        output_file: destination.to_string_lossy().to_string(),
        input_size_bytes: file_len,
        output_size_bytes: plaintext.len() as u64,
        cipher: "AES-256-GCM".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Shredding
// ---------------------------------------------------------------------------

/// Overwrite a file three times, truncate it, rename it and unlink it.
///
/// Pass three uses the operating system's entropy source, not a counter: the
/// old `aegis` wrote a deterministic sequence derived from the file size, which
/// an adversary who knows the size can reproduce.
///
/// With `dry_run` nothing is touched.
pub fn shred(path: &Path, dry_run: bool) -> Result<ShredReport> {
    if !path.is_file() {
        return Err(Error::NotFound(path.display().to_string()));
    }

    let original_size = fs::metadata(path)?.len();

    if dry_run {
        return Ok(ShredReport {
            original_path: path.to_string_lossy().to_string(),
            original_size_bytes: original_size,
            passes_completed: 0,
            method: "DoD 5220.22-M (3 passes)".to_string(),
            file_unlinked: false,
            dry_run: true,
        });
    }

    let mut file = OpenOptions::new().read(true).write(true).open(path)?;

    for pass in 0..3 {
        file.seek(SeekFrom::Start(0))?;

        let mut buffer = vec![0u8; CHUNK];
        let mut remaining = original_size;

        while remaining > 0 {
            let take = (remaining as usize).min(CHUNK);
            match pass {
                0 => buffer[..take].fill(0x00),
                1 => buffer[..take].fill(0xFF),
                _ => thread_rng().fill_bytes(&mut buffer[..take]),
            }
            file.write_all(&buffer[..take])?;
            remaining -= take as u64;
        }
        file.flush()?;
    }

    file.set_len(0)?;
    file.sync_all()?;
    drop(file);

    // Rename before unlinking: the old name should not survive either.
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let scrambled: PathBuf = parent.join(format!("shredded_{:016X}.tmp", thread_rng().next_u64()));

    let target = match fs::rename(path, &scrambled) {
        Ok(()) => scrambled,
        // If the rename fails, still remove the original rather than leaving
        // a zeroed file with its old name in place.
        Err(_) => path.to_path_buf(),
    };

    fs::remove_file(&target)?;

    Ok(ShredReport {
        original_path: path.to_string_lossy().to_string(),
        original_size_bytes: original_size,
        passes_completed: 3,
        method: "DoD 5220.22-M (3 passes)".to_string(),
        file_unlinked: true,
        dry_run: false,
    })
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

/// Load a signature database: a JSON array of `{ "name": ..., "crc32": ... }`.
pub fn load_signatures(path: &Path) -> Result<Vec<Signature>> {
    let text = fs::read_to_string(path)?;
    let parsed: Vec<Signature> = serde_json::from_str(&text)
        .map_err(|e| Error::Invalid(format!("not a signature file: {}", e)))?;
    Ok(parsed)
}

const SUSPICIOUS_TAILS: &[&str] = &[
    ".pdf.exe",
    ".doc.exe",
    ".txt.exe",
    ".jpg.exe",
    ".png.exe",
    ".pdf.scr",
    ".doc.vbs",
    ".xls.exe",
];

fn inspect_file(path: &Path, signatures: &[Signature], threats: &mut Vec<ThreatItem>) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let shown = path.to_string_lossy().to_string();

    if name == "autorun.inf" {
        threats.push(ThreatItem {
            file_path: shown.clone(),
            threat_name: "Heuristic.SuspiciousAutorunInf".to_string(),
            threat_type: "Heuristic".to_string(),
            severity: "High".to_string(),
            detail: "autorun.inf can start a program when a removable drive is mounted"
                .to_string(),
        });
    }

    for tail in SUSPICIOUS_TAILS {
        if name.ends_with(tail) {
            threats.push(ThreatItem {
                file_path: shown.clone(),
                threat_name: "Heuristic.DeceptiveDoubleExtension".to_string(),
                threat_type: "Heuristic".to_string(),
                severity: "Critical".to_string(),
                detail: format!("'{}' disguises an executable behind a document extension", tail),
            });
        }
    }

    if !signatures.is_empty() {
        if let Ok(data) = fs::read(path) {
            let sum = crc32(&data);
            for sig in signatures {
                if sig.crc32 == sum {
                    threats.push(ThreatItem {
                        file_path: shown.clone(),
                        threat_name: format!("Signature.{}", sig.name),
                        threat_type: "Signature".to_string(),
                        severity: "Critical".to_string(),
                        detail: format!("exact match against the signature database (crc32 0x{:08X})", sum),
                    });
                }
            }
        }
    }
}

/// Scan a file or a tree for the heuristics above, and for any supplied
/// signatures.
pub fn scan(target: &Path, signatures: &[Signature]) -> Result<ScanReport> {
    if !target.exists() {
        return Err(Error::NotFound(target.display().to_string()));
    }

    let mut files_scanned = 0usize;
    let mut threats = Vec::new();

    let files = savehorse_core::walk_files(
        target,
        &savehorse_core::WalkOptions {
            include_hidden: true,
            ..Default::default()
        },
    )?;

    for file in files {
        files_scanned += 1;
        inspect_file(&file, signatures, &mut threats);
    }

    Ok(ScanReport {
        target_path: target.to_string_lossy().to_string(),
        files_scanned,
        threats_found: threats.len(),
        clean: threats.is_empty(),
        signatures_loaded: signatures.len(),
        threats,
    })
}

// ---------------------------------------------------------------------------
// Hardening
// ---------------------------------------------------------------------------

/// Re-exported from the binary lab: one PE parser for the whole workspace.
pub use savehorse_forge::{security_flags, SecurityFlags};

/// Read a PE's hardening flags.
pub fn harden(path: &Path) -> Result<SecurityFlags> {
    security_flags(path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-aegis-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let dir = scratch("crypto");
        let source = dir.join("secret.txt");
        fs::write(&source, b"the quick brown fox").unwrap();

        let container = dir.join("secret.vault");
        let encrypted = encrypt(&source, Some(&container), Some("correct horse"), None).unwrap();
        assert_eq!(encrypted.cipher, "AES-256-GCM");
        assert!(container.exists());

        // The ciphertext must not contain the plaintext.
        let raw = fs::read(&container).unwrap();
        assert!(!raw.windows(9).any(|w| w == b"quick bro"));

        let restored = dir.join("restored.txt");
        let decrypted = decrypt(&container, Some(&restored), Some("correct horse"), None).unwrap();
        assert_eq!(decrypted.output_size_bytes, 19);
        assert_eq!(fs::read(&restored).unwrap(), b"the quick brown fox");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_passphrase_is_refused() {
        let dir = scratch("wrong-pass");
        let source = dir.join("a.txt");
        fs::write(&source, b"data").unwrap();
        let container = dir.join("a.vault");
        encrypt(&source, Some(&container), Some("right"), None).unwrap();

        let err = decrypt(&container, Some(&dir.join("out.txt")), Some("wrong"), None).unwrap_err();
        assert!(err.to_string().contains("could not decrypt"), "got: {}", err);
        assert!(!dir.join("out.txt").exists(), "nothing must be written");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_byte_is_caught() {
        let dir = scratch("tamper");
        let source = dir.join("a.txt");
        fs::write(&source, b"data to protect").unwrap();
        let container = dir.join("a.vault");
        encrypt(&source, Some(&container), Some("pw"), None).unwrap();

        let mut bytes = fs::read(&container).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(&container, &bytes).unwrap();

        assert!(decrypt(&container, Some(&dir.join("out")), Some("pw"), None).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_encryptions_of_one_file_differ() {
        let dir = scratch("salt");
        let source = dir.join("a.txt");
        fs::write(&source, b"same input").unwrap();

        let one = dir.join("one.vault");
        let two = dir.join("two.vault");
        encrypt(&source, Some(&one), Some("pw"), None).unwrap();
        encrypt(&source, Some(&two), Some("pw"), None).unwrap();

        assert_ne!(
            fs::read(&one).unwrap(),
            fs::read(&two).unwrap(),
            "a random salt and nonce must make the containers differ"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_keyfile_works_instead_of_a_passphrase() {
        let dir = scratch("keyfile");
        let source = dir.join("a.txt");
        fs::write(&source, b"keyfile secret").unwrap();
        let keyfile = dir.join("key.bin");
        fs::write(&keyfile, b"0123456789abcdef").unwrap();

        let container = dir.join("a.vault");
        encrypt(&source, Some(&container), None, Some(&keyfile)).unwrap();

        let out = dir.join("out.txt");
        decrypt(&container, Some(&out), None, Some(&keyfile)).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"keyfile secret");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn encryption_needs_a_secret() {
        let dir = scratch("no-secret");
        let source = dir.join("a.txt");
        fs::write(&source, b"x").unwrap();
        assert!(encrypt(&source, Some(&dir.join("o.vault")), None, None).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shred_previews_without_touching_anything() {
        let dir = scratch("shred-preview");
        let file = dir.join("doomed.txt");
        fs::write(&file, b"please keep me").unwrap();

        let report = shred(&file, true).unwrap();
        assert!(report.dry_run);
        assert_eq!(report.passes_completed, 0);
        assert_eq!(fs::read(&file).unwrap(), b"please keep me");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shred_overwrites_then_removes() {
        let dir = scratch("shred");
        let file = dir.join("doomed.txt");
        fs::write(&file, b"please keep me").unwrap();

        let report = shred(&file, false).unwrap();
        assert_eq!(report.passes_completed, 3);
        assert!(report.file_unlinked);
        assert!(!file.exists(), "the file must be gone");
        // Nothing left behind in the directory either.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_flags_an_autorun_and_a_double_extension() {
        let dir = scratch("scan");
        fs::write(dir.join("autorun.inf"), b"[autorun]\nopen=x.exe").unwrap();
        fs::write(dir.join("invoice.pdf.exe"), b"MZ").unwrap();
        fs::write(dir.join("ordinary.txt"), b"nothing to see").unwrap();

        let report = scan(&dir, &[]).unwrap();
        assert_eq!(report.files_scanned, 3);
        assert_eq!(report.threats_found, 2);
        assert!(!report.clean);

        let names: Vec<&str> = report.threats.iter().map(|t| t.threat_name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("Autorun")));
        assert!(names.iter().any(|n| n.contains("DoubleExtension")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_matches_a_supplied_signature() {
        let dir = scratch("signature");
        let payload = b"pretend this is malware";
        fs::write(dir.join("sample.bin"), payload).unwrap();

        let signatures = vec![Signature {
            name: "Test.Sample".to_string(),
            crc32: crc32(payload),
        }];

        let report = scan(&dir, &signatures).unwrap();
        assert_eq!(report.signatures_loaded, 1);
        assert_eq!(report.threats_found, 1);
        assert!(report.threats[0].threat_name.contains("Test.Sample"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_of_a_clean_tree_is_clean() {
        let dir = scratch("clean");
        fs::write(dir.join("a.txt"), b"fine").unwrap();
        fs::write(dir.join("b.txt"), b"also fine").unwrap();

        let report = scan(&dir, &[]).unwrap();
        assert!(report.clean);
        assert_eq!(report.files_scanned, 2);
        assert!(report.threats.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
