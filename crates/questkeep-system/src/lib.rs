//! Hardware and system details for the statistics page: OS, CPU, memory, GPUs, displays and drives.
//!
//! [`system_info`] never fails; whatever cannot be read is `None` or an empty list. Everything
//! comes from `sysinfo`, plus Win32 calls (registry, SetupAPI, display config, storage IOCTLs) on
//! Windows for what `sysinfo` does not cover.

use serde::Serialize;
use ts_rs::TS;

mod common;
#[cfg(windows)]
mod windows;

/// CPU model and core counts.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CpuInfo {
    pub name: String,
    pub cores: u32,
    pub threads: u32,
}

/// A graphics adapter.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GpuInfo {
    pub name: String,
    #[ts(type = "number | null")]
    pub memory_bytes: Option<u64>,
    pub driver_version: Option<String>,
    /// "YYYY-MM-DD"
    pub driver_date: Option<String>,
}

/// An active display (monitor).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DisplayInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
    /// None: unknown
    pub hdr: Option<bool>,
    pub primary: bool,
}

/// What kind of storage a drive is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DriveKind {
    Nvme,
    Ssd,
    Hdd,
    Removable,
    Network,
    Unknown,
}

/// A mounted drive or volume.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DriveInfo {
    /// "C:\\" or "/"
    pub mount: String,
    pub label: Option<String>,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub free_bytes: u64,
    pub kind: DriveKind,
    pub file_system: Option<String>,
}

/// Everything the statistics page shows about this PC.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SystemInfo {
    /// "Windows 11 Pro 25H2", "Ubuntu 24.04", ...
    pub os: String,
    pub cpu: Option<CpuInfo>,
    #[ts(type = "number")]
    pub memory_bytes: u64,
    #[ts(type = "number")]
    pub uptime_seconds: u64,
    pub gpus: Vec<GpuInfo>,
    pub displays: Vec<DisplayInfo>,
    pub drives: Vec<DriveInfo>,
}

/// Reads everything; never fails (missing parts are `None` or empty).
pub fn system_info() -> SystemInfo {
    let basics = common::read_basics();
    #[cfg(windows)]
    let (os, gpus, displays) = (
        windows::os_name().unwrap_or(basics.os),
        windows::gpus(),
        windows::displays(),
    );
    #[cfg(not(windows))]
    let (os, gpus, displays) = (basics.os, Vec::new(), Vec::new());

    #[allow(unused_mut)]
    let mut drives = basics.drives;
    #[cfg(windows)]
    windows::refine_drives(&mut drives);

    SystemInfo {
        os,
        cpu: basics.cpu,
        memory_bytes: basics.memory_bytes,
        uptime_seconds: basics.uptime_seconds,
        gpus,
        displays,
        drives,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_info_smoke() {
        let info = system_info();
        assert!(info.memory_bytes > 0);
        assert!(!info.os.is_empty());
    }

    #[test]
    fn drive_kind_serializes_camel_case() {
        assert_eq!(
            serde_json::to_string(&DriveKind::Nvme).unwrap_or_default(),
            "\"nvme\""
        );
    }
}
