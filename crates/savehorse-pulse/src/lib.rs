//! pulse — the system module.
//!
//! From `pulse` 1.0.0 (generation 2) and `beacon` 1.0.0 (generation 1).
//!
//! The two operations here need nothing from the platform: a file hash, and an
//! audit of how much space the temporary directory is holding. They are the
//! portable core of the module.
//!
//! Hardware, processes and drives come from `sysinfo`, which replaces the
//! hand-rolled Win32 calls the original made: the same information, on three
//! platforms, from one dependency.
//!
//! `services` asks the platform's own service manager, because there is no
//! portable API for it and inventing one would be worse than shelling out to
//! the tool that already knows.
//!
//! `priority` is still declared but not ported. Raising a process's priority
//! needs a privileged platform call on every operating system, and a wrong
//! implementation is a good way to make a machine unusable.

use savehorse_core::{crc32, format_bytes, sha256_file, Error, Result, WalkOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct HashReport {
    pub file: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub crc32: String,
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TempEntry {
    pub path: String,
    pub size_bytes: u64,
    pub size_human: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TempReport {
    pub directory: String,
    pub files: usize,
    pub total_bytes: u64,
    pub total_human: String,
    /// The largest files, biggest first — what is actually worth clearing.
    pub largest: Vec<TempEntry>,
}

/// CRC-32 and SHA-256 of a file, which is what you need to compare two copies
/// or to check one against a published value.
pub fn hash_file(path: &Path) -> Result<HashReport> {
    let data = std::fs::read(path)?;
    let size = data.len() as u64;

    Ok(HashReport {
        file: path.to_string_lossy().to_string(),
        size_bytes: size,
        size_human: format_bytes(size),
        crc32: format!("{:08x}", crc32(&data)),
        sha256: sha256_file(path)?,
    })
}

/// Measure what a directory is holding, without deleting anything.
///
/// The default target is the operating system's temporary directory. Reading
/// the tree is a scan, not a cleanup: nothing here removes a file.
pub fn audit_temp(directory: Option<&Path>, top: usize) -> Result<TempReport> {
    let root: PathBuf = match directory {
        Some(p) => p.to_path_buf(),
        None => std::env::temp_dir(),
    };

    if !root.is_dir() {
        return Err(Error::Invalid(format!(
            "'{}' is not a directory",
            root.display()
        )));
    }

    let files = savehorse_core::walk_files(
        &root,
        &WalkOptions {
            include_hidden: true,
            ..Default::default()
        },
    )?;

    let mut total = 0u64;
    let mut entries: Vec<TempEntry> = Vec::with_capacity(files.len());

    for file in &files {
        let size = match std::fs::metadata(file) {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        total += size;
        entries.push(TempEntry {
            path: file.to_string_lossy().to_string(),
            size_bytes: size,
            size_human: format_bytes(size),
        });
    }

    entries.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    entries.truncate(top.max(1));

    Ok(TempReport {
        directory: root.to_string_lossy().to_string(),
        files: files.len(),
        total_bytes: total,
        total_human: format_bytes(total),
        largest: entries,
    })
}


// ---------------------------------------------------------------------------
// Hardware, processes, drives and services
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct HardwareReport {
    pub hostname: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub kernel: Option<String>,
    pub arch: String,
    pub cpu_brand: String,
    pub cpu_cores_logical: usize,
    pub cpu_cores_physical: Option<usize>,
    pub cpu_frequency_mhz: u64,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub total_swap_bytes: u64,
    pub used_swap_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
    pub memory_human: String,
    pub cpu_percent: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProcessReport {
    pub total_processes: usize,
    pub shown: usize,
    /// How the list was ordered, so a reader knows what "top" means.
    pub sorted_by: String,
    pub rows: Vec<ProcessRow>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DriveRow {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub kind: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: u64,
    pub removable: bool,
    pub read_only: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ServiceRow {
    pub name: String,
    pub state: String,
    pub detail: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ServiceReport {
    pub platform: String,
    /// The command whose output was read, so the numbers can be checked.
    pub source: String,
    pub total: usize,
    pub services: Vec<ServiceRow>,
}

fn system() -> Result<sysinfo::System> {
    sysinfo::System::new_all()
        .map_err(|e| Error::Invalid(format!("could not read system information: {}", e)))
}

/// Processor, memory, host and operating system.
pub fn hardware() -> Result<HardwareReport> {
    let sys = system()?;

    let cpus = sys.cpus();
    let brand = cpus
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let frequency = cpus.first().map(|c| c.frequency()).unwrap_or(0);

    Ok(HardwareReport {
        hostname: sysinfo::System::host_name().ok(),
        os_name: sysinfo::System::name().ok(),
        os_version: sysinfo::System::os_version().ok(),
        kernel: sysinfo::System::kernel_version().ok(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_brand: brand,
        cpu_cores_logical: cpus.len(),
        cpu_cores_physical: sysinfo::System::physical_core_count().ok(),
        cpu_frequency_mhz: frequency,
        total_memory_bytes: sys.total_memory(),
        used_memory_bytes: sys.used_memory(),
        total_swap_bytes: sys.total_swap(),
        used_swap_bytes: sys.used_swap(),
    })
}

/// The busiest processes, by memory.
///
/// CPU percentage is measured across two refreshes with a short gap between
/// them, because a single sample of a cumulative counter is always zero.
pub fn processes(top: usize) -> Result<ProcessReport> {
    let mut sys = system()?;

    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_all();

    let total = sys.processes().len();
    let mut rows: Vec<ProcessRow> = sys
        .processes()
        .iter()
        .map(|(pid, process)| {
            let memory = process.memory();
            ProcessRow {
                pid: pid.as_u32(),
                name: process.name().to_string_lossy().to_string(),
                memory_bytes: memory,
                memory_human: format_bytes(memory),
                cpu_percent: process.cpu_usage(),
            }
        })
        .collect();

    rows.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
    rows.truncate(top.max(1));

    Ok(ProcessReport {
        total_processes: total,
        shown: rows.len(),
        sorted_by: "memory, largest first".to_string(),
        rows,
    })
}

/// Mounted volumes and how full they are.
pub fn drives() -> Result<Vec<DriveRow>> {
    let disks = sysinfo::Disks::new_with_refreshed_list()
        .map_err(|e| Error::Invalid(format!("could not read the disk list: {}", e)))?;

    let mut rows: Vec<DriveRow> = disks
        .iter()
        .map(|disk| {
            let total = disk.total_space();
            let available = disk.available_space();
            let used = total.saturating_sub(available);
            DriveRow {
                name: disk.name().to_string_lossy().to_string(),
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                file_system: disk.file_system().to_string_lossy().to_string(),
                kind: format!("{:?}", disk.kind()),
                total_bytes: total,
                available_bytes: available,
                used_bytes: used,
                used_percent: if total > 0 { used * 100 / total } else { 0 },
                removable: disk.is_removable(),
                read_only: disk.is_read_only(),
            }
        })
        .collect();

    rows.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    Ok(rows)
}

/// Ask the platform's service manager what is installed.
///
/// Read-only. Nothing is started, stopped or enabled.
pub fn services() -> Result<ServiceReport> {
    let platform = std::env::consts::OS.to_string();

    let (command, args, source): (&str, Vec<&str>, String) = match platform.as_str() {
        "windows" => (
            "sc",
            vec!["query", "type=", "service", "state=", "all"],
            "sc query type= service state= all".to_string(),
        ),
        "macos" => ("launchctl", vec!["list"], "launchctl list".to_string()),
        _ => (
            "systemctl",
            vec![
                "list-units",
                "--type=service",
                "--all",
                "--no-pager",
                "--no-legend",
                "--plain",
            ],
            "systemctl list-units --type=service --all".to_string(),
        ),
    };

    let output = match std::process::Command::new(command).args(&args).output() {
        Ok(output) => output,
        // A container without systemd is not an error: it is a machine with no
        // service manager, and the report says which command was missing.
        Err(e) => {
            return Ok(ServiceReport {
                platform,
                source: format!("{} (not available: {})", source, e),
                total: 0,
                services: Vec::new(),
            })
        }
    };

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let services = parse_services(&platform, &text);

    Ok(ServiceReport {
        platform,
        source,
        total: services.len(),
        services,
    })
}

fn parse_services(platform: &str, text: &str) -> Vec<ServiceRow> {
    let mut rows = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match platform {
            // name  load  active  sub  description
            "windows" => {
                // sc prints "SERVICE_NAME: foo" and "STATE : 4  RUNNING".
                if let Some(name) = line.strip_prefix("SERVICE_NAME:") {
                    rows.push(ServiceRow {
                        name: name.trim().to_string(),
                        state: String::new(),
                        detail: String::new(),
                    });
                } else if line.starts_with("STATE") {
                    if let Some(last) = rows.last_mut() {
                        let value = line.split(':').nth(1).unwrap_or("").trim();
                        let mut parts = value.split_whitespace();
                        let _code = parts.next();
                        last.state = parts.next().unwrap_or("unknown").to_string();
                    }
                }
            }
            // PID  Status  Label
            "macos" => {
                let mut parts = line.split_whitespace();
                let pid = parts.next().unwrap_or("");
                let status = parts.next().unwrap_or("");
                let label = parts.next().unwrap_or("");
                if label.is_empty() || pid == "PID" {
                    continue;
                }
                rows.push(ServiceRow {
                    name: label.to_string(),
                    state: if pid == "-" {
                        "not running".to_string()
                    } else {
                        format!("running (pid {})", pid)
                    },
                    detail: format!("last exit status {}", status),
                });
            }
            // unit  load  active  sub  description
            _ => {
                let mut parts = line.split_whitespace();
                let name = parts.next().unwrap_or("");
                if !name.contains(".service") {
                    continue;
                }
                let _load = parts.next().unwrap_or("");
                let active = parts.next().unwrap_or("");
                let sub = parts.next().unwrap_or("");
                let description = parts.collect::<Vec<_>>().join(" ");
                rows.push(ServiceRow {
                    name: name.to_string(),
                    state: format!("{} ({})", active, sub),
                    detail: description,
                });
            }
        }
    }

    rows
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-pulse-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn hashes_a_file_both_ways() {
        let dir = scratch("hash");
        let file = dir.join("a.txt");
        fs::write(&file, b"abc").unwrap();

        let report = hash_file(&file).unwrap();
        assert_eq!(report.size_bytes, 3);
        assert_eq!(
            report.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // The IEEE CRC-32 of "abc" is 0x352441C2.
        assert_eq!(report.crc32, "352441c2");
        let _ = fs::remove_dir_all(&dir);
    }


    #[test]
    fn reports_plausible_hardware() {
        let report = hardware().unwrap();
        assert!(report.cpu_cores_logical >= 1, "at least one logical core");
        assert!(report.total_memory_bytes > 0, "memory is not zero");
        assert!(!report.arch.is_empty());
        assert!(report.used_memory_bytes <= report.total_memory_bytes);
    }

    #[test]
    fn lists_processes_by_memory() {
        let report = processes(5).unwrap();
        assert!(report.total_processes >= 1);
        assert!(report.shown <= 5);
        assert_eq!(report.shown, report.rows.len());

        // Descending by memory.
        for pair in report.rows.windows(2) {
            assert!(pair[0].memory_bytes >= pair[1].memory_bytes);
        }
    }

    #[test]
    fn lists_at_least_one_drive() {
        let report = drives().unwrap();
        assert!(!report.is_empty(), "a running machine has a filesystem");
        for drive in &report {
            assert!(drive.used_bytes <= drive.total_bytes);
            assert!(drive.used_percent <= 100);
        }
    }

    #[test]
    fn services_never_errors_when_the_manager_is_absent() {
        // The command may not exist in a container. That is a report with a
        // reason attached, not a failure.
        let report = services().unwrap();
        assert!(!report.platform.is_empty());
        assert!(!report.source.is_empty());
        assert_eq!(report.total, report.services.len());
    }

    #[test]
    fn parses_systemd_output() {
        let text = "cron.service loaded active running Regular background program\n\
                    ssh.service loaded inactive dead OpenBSD Secure Shell server\n\
                    not-a-unit loaded active running ignored\n";
        let rows = parse_services("linux", text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "cron.service");
        assert_eq!(rows[0].state, "active (running)");
        assert!(rows[0].detail.contains("background"));
    }

    #[test]
    fn parses_launchctl_output() {
        let text = "PID\tStatus\tLabel\n123\t0\tcom.example.agent\n-\t0\tcom.example.idle\n";
        let rows = parse_services("macos", text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state, "running (pid 123)");
        assert_eq!(rows[1].state, "not running");
    }

    #[test]
    fn parses_sc_output() {
        let text = "SERVICE_NAME: Spooler\n        TYPE               : 110  WIN32_OWN_PROCESS\n\
                    STATE              : 4  RUNNING\n";
        let rows = parse_services("windows", text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Spooler");
        assert_eq!(rows[0].state, "RUNNING");
    }

    #[test]
    fn audit_counts_and_ranks_without_deleting() {
        let dir = scratch("temp");
        fs::write(dir.join("small.txt"), b"x").unwrap();
        fs::write(dir.join("big.txt"), vec![0u8; 2048]).unwrap();
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("mid.txt"), vec![0u8; 100]).unwrap();

        let report = audit_temp(Some(&dir), 2).unwrap();
        assert_eq!(report.files, 3);
        assert_eq!(report.total_bytes, 2149);
        assert_eq!(report.largest.len(), 2, "top two only");
        assert!(report.largest[0].path.ends_with("big.txt"), "biggest first");

        // An audit is a read: everything is still there.
        assert!(dir.join("big.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn audit_rejects_a_path_that_is_not_a_directory() {
        let dir = scratch("temp-bad");
        let file = dir.join("a.txt");
        fs::write(&file, b"x").unwrap();
        assert!(audit_temp(Some(&file), 5).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
