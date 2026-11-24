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
//! `priority` changes a process's scheduling class. It previews by default:
//! `--dry-run` reports what would change and touches nothing, and raising a
//! priority above normal needs privileges on every platform, which is reported
//! rather than assumed.

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
        file: savehorse_core::display_path(&path),
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
            savehorse_core::display_path(&root)
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
            path: savehorse_core::display_path(&file),
            size_bytes: size,
            size_human: format_bytes(size),
        });
    }

    entries.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    entries.truncate(top.max(1));

    Ok(TempReport {
        directory: savehorse_core::display_path(&root),
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
// Process priority
// ---------------------------------------------------------------------------

/// The scheduling classes, in the order the help lists them, from least to most
/// demanding of the machine.
pub const PRIORITY_LEVELS: &[&str] = &[
    "idle",
    "below-normal",
    "normal",
    "above-normal",
    "high",
    "realtime",
];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PriorityReport {
    pub pid: u32,
    pub level: String,
    pub applied: bool,
    /// The platform's own name for the class, or the nice value on Unix.
    pub native: String,
    pub detail: String,
}

/// What a level means on this platform.
///
/// Unix uses niceness, where a larger number is a *lower* priority and only a
/// privileged process may go negative. Windows uses a priority class, and every
/// value is allowed but only some are wise.
fn native_for(level: &str) -> Result<(&'static str, i64)> {
    let pair = match level.trim().to_lowercase().as_str() {
        "idle" => ("IDLE_PRIORITY_CLASS", 19),
        "below-normal" => ("BELOW_NORMAL_PRIORITY_CLASS", 10),
        "normal" => ("NORMAL_PRIORITY_CLASS", 0),
        "above-normal" => ("ABOVE_NORMAL_PRIORITY_CLASS", -5),
        "high" => ("HIGH_PRIORITY_CLASS", -10),
        "realtime" => ("REALTIME_PRIORITY_CLASS", -20),
        other => {
            return Err(Error::Invalid(format!(
                "unknown priority '{}': use {}",
                other,
                PRIORITY_LEVELS.join(", ")
            )))
        }
    };
    Ok(pair)
}

/// How this platform names the level: a priority class on Windows, a nice
/// value on Unix. Showing `IDLE_PRIORITY_CLASS` on Linux would be a lie about
/// what the tool just asked the kernel to do.
fn describe(level: &str) -> Result<String> {
    let (name, nice) = native_for(level)?;
    if cfg!(windows) {
        Ok(name.to_string())
    } else {
        Ok(format!("nice {}", nice))
    }
}

/// True when the level is above normal, which needs privileges on Unix.
pub fn needs_privilege(level: &str) -> bool {
    matches!(
        level.trim().to_lowercase().as_str(),
        "above-normal" | "high" | "realtime"
    )
}

#[cfg(windows)]
fn apply(pid: u32, level: &str) -> Result<String> {
    use std::ffi::c_void;

    type Handle = isize;
    const PROCESS_SET_INFORMATION: u32 = 0x0200;
    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn SetPriorityClass(process: Handle, class: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    let class: u32 = match level.trim().to_lowercase().as_str() {
        "idle" => 0x0000_0040,
        "below-normal" => 0x0000_4000,
        "normal" => 0x0000_0020,
        "above-normal" => 0x0000_8000,
        "high" => 0x0000_0080,
        "realtime" => 0x0000_0100,
        other => {
            return Err(Error::Invalid(format!(
                "unknown priority '{}': use {}",
                other,
                PRIORITY_LEVELS.join(", ")
            )))
        }
    };

    unsafe {
        let handle = OpenProcess(
            PROCESS_SET_INFORMATION | PROCESS_QUERY_INFORMATION,
            0,
            pid,
        );
        if handle == 0 {
            return Err(Error::Invalid(format!(
                "could not open process {}: it may not exist, or it may need \
                 administrator rights",
                pid
            )));
        }
        let ok = SetPriorityClass(handle, class);
        CloseHandle(handle);
        let _ = std::mem::size_of::<*mut c_void>();
        if ok == 0 {
            return Err(Error::Invalid(format!(
                "the system refused to change process {} to {}",
                pid, level
            )));
        }
    }

    describe(level)
}

#[cfg(unix)]
fn apply(pid: u32, level: &str) -> Result<String> {
    let (_, nice) = native_for(level)?;
    let result = unsafe {
        // PRIO_PROCESS asks for the priority of the process with this id.
        libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice as libc::c_int)
    };

    if result != 0 {
        let err = std::io::Error::last_os_error();
        return Err(Error::Invalid(format!(
            "the system refused to set nice {} on process {}: {}{}",
            nice,
            pid,
            err,
            if needs_privilege(level) {
                " — lowering niceness below 0 needs root"
            } else {
                ""
            }
        )));
    }

    Ok(format!("nice {}", nice))
}

#[cfg(not(any(windows, unix)))]
fn apply(_pid: u32, _level: &str) -> Result<String> {
    Err(Error::Invalid(
        "changing a process priority is not supported on this platform".to_string(),
    ))
}

/// Change a process's scheduling class.
///
/// `dry_run` reports what would happen and changes nothing, which is the
/// default: this operation is declared destructive in the manifest, and moving
/// a process to a higher priority can make a machine unusable.
pub fn set_priority(pid: u32, level: &str, dry_run: bool) -> Result<PriorityReport> {
    native_for(level)?;

    if pid == 0 {
        return Err(Error::Invalid(
            "pid 0 is the scheduler itself on Unix, not a process to change".to_string(),
        ));
    }

    if dry_run {
        return Ok(PriorityReport {
            pid,
            level: level.to_string(),
            applied: false,
            native: describe(level)?,
            detail: "dry run: nothing was changed".to_string(),
        });
    }

    let native = apply(pid, level)?;
    Ok(PriorityReport {
        pid,
        level: level.to_string(),
        applied: true,
        native,
        detail: format!("process {} is now {}", pid, level),
    })
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
    fn every_documented_priority_maps() {
        for level in PRIORITY_LEVELS {
            assert!(native_for(level).is_ok(), "level '{}' should be valid", level);
        }
        assert!(native_for("urgent").is_err());
    }

    #[test]
    fn niceness_runs_from_idle_up_to_realtime() {
        let idle = native_for("idle").unwrap().1;
        let normal = native_for("normal").unwrap().1;
        let realtime = native_for("realtime").unwrap().1;
        assert_eq!(normal, 0, "normal is zero on Unix");
        assert!(idle > normal, "idle is a larger nice than normal");
        assert!(realtime < normal, "realtime is a negative nice");
    }

    #[test]
    fn the_levels_above_normal_need_privileges() {
        assert!(!needs_privilege("idle"));
        assert!(!needs_privilege("normal"));
        assert!(needs_privilege("above-normal"));
        assert!(needs_privilege("high"));
        assert!(needs_privilege("realtime"));
    }

    #[test]
    fn a_dry_run_reports_and_changes_nothing() {
        // The current process, so the pid certainly exists.
        let pid = std::process::id();
        let report = set_priority(pid, "idle", true).unwrap();
        assert_eq!(report.pid, pid);
        assert!(!report.applied, "a dry run must not apply");
        assert!(report.detail.contains("nothing was changed"));
        assert!(
            report.native.contains("19") || report.native.contains("IDLE"),
            "the native name should describe this platform, got: {}",
            report.native
        );
    }

    #[test]
    fn pid_zero_is_refused() {
        assert!(set_priority(0, "normal", false).is_err());
        assert!(set_priority(0, "normal", true).is_err());
    }

    #[test]
    fn an_unknown_level_is_refused_before_anything_is_touched() {
        let err = set_priority(std::process::id(), "urgent", false).unwrap_err();
        assert!(err.to_string().contains("unknown priority"), "got: {}", err);
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
