//! The cross-platform part: `sysinfo` reads, and the pure helpers around it.
// Some helpers are only used by the Windows module.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashSet;

use sysinfo::{CpuRefreshKind, DiskKind, Disks, MemoryRefreshKind, RefreshKind, System};

use crate::{CpuInfo, DriveInfo, DriveKind};

pub(crate) struct Basics {
    pub os: String,
    pub cpu: Option<CpuInfo>,
    pub memory_bytes: u64,
    pub uptime_seconds: u64,
    pub drives: Vec<DriveInfo>,
}

pub(crate) fn read_basics() -> Basics {
    let sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing())
            .with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    let threads = u32::try_from(sys.cpus().len()).unwrap_or(0);
    let cpu = sys.cpus().first().map(|c| CpuInfo {
        name: clean_cpu_name(c.brand()),
        cores: System::physical_core_count()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(threads),
        threads,
    });
    let os = System::long_os_version()
        .or_else(System::name)
        .unwrap_or_else(|| "Unknown".to_string());
    Basics {
        os,
        cpu: cpu.filter(|c| !c.name.is_empty()),
        memory_bytes: sys.total_memory(),
        uptime_seconds: System::uptime(),
        drives: read_drives(),
    }
}

fn read_drives() -> Vec<DriveInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in disks.list() {
        let mount = d.mount_point().to_string_lossy().into_owned();
        let fs = d.file_system().to_string_lossy().into_owned();
        if d.total_space() == 0 || is_pseudo(&mount, &fs) || !seen.insert(mount.clone()) {
            continue;
        }
        let kind = if d.is_removable() {
            DriveKind::Removable
        } else {
            match d.kind() {
                DiskKind::SSD => DriveKind::Ssd,
                DiskKind::HDD => DriveKind::Hdd,
                _ => DriveKind::Unknown,
            }
        };
        out.push(DriveInfo {
            mount,
            label: non_empty(&d.name().to_string_lossy()),
            total_bytes: d.total_space(),
            free_bytes: d.available_space(),
            kind,
            file_system: non_empty(&fs),
        });
    }
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out
}

pub(crate) fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// File systems and mounts that are not real disks (Linux pseudo and container mounts).
pub(crate) fn is_pseudo(mount: &str, fs: &str) -> bool {
    const FS: &[&str] = &[
        "tmpfs",
        "devtmpfs",
        "proc",
        "sysfs",
        "cgroup",
        "cgroup2",
        "overlay",
        "squashfs",
        "devpts",
        "securityfs",
        "debugfs",
        "tracefs",
        "configfs",
        "fusectl",
        "pstore",
        "bpf",
        "autofs",
        "mqueue",
        "hugetlbfs",
        "ramfs",
        "efivarfs",
        "binfmt_misc",
    ];
    const PREFIXES: &[&str] = &["/proc", "/sys", "/dev", "/run", "/snap", "/var/lib/docker"];
    FS.contains(&fs)
        || PREFIXES
            .iter()
            .any(|p| mount == *p || mount.starts_with(&format!("{p}/")))
}

/// CPU brand strings are space padded and sometimes repeat whitespace.
pub(crate) fn clean_cpu_name(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Windows' `ProductName` still says "Windows 10" on Windows 11 (build >= 22000); appends the
/// display version ("25H2") when there is one.
pub(crate) fn windows_os_name(
    product: &str,
    build: Option<u32>,
    display_version: Option<&str>,
) -> String {
    let mut name = product.trim().to_string();
    if build.is_some_and(|b| b >= 22000) {
        name = name.replacen("Windows 10", "Windows 11", 1);
    }
    match display_version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => format!("{name} {v}"),
        None => name,
    }
}

/// "M-D-YYYY" (the driver registry format) to "YYYY-MM-DD". Also accepts "M/D/YYYY".
pub(crate) fn driver_date(raw: &str) -> Option<String> {
    let mut parts = raw.trim().split(['-', '/']);
    let m: u32 = parts.next()?.trim().parse().ok()?;
    let d: u32 = parts.next()?.trim().parse().ok()?;
    let y: u32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1980 {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// Refresh rate from a numerator/denominator pair, rounded to two decimals.
pub(crate) fn refresh_hz(numerator: u32, denominator: u32) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    (f64::from(numerator) / f64::from(denominator) * 100.0).round() / 100.0
}

/// Decodes a little-endian 4 or 8 byte value (registry DWORD/QWORD/binary memory sizes).
pub(crate) fn le_u64(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        4 => Some(u64::from(u32::from_le_bytes(bytes.try_into().ok()?))),
        8 => Some(u64::from_le_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}

/// Decodes a registry string (UTF-16LE bytes, usually NUL terminated).
pub(crate) fn utf16_bytes_to_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

// Storage bus types from `STORAGE_BUS_TYPE`.
pub(crate) const BUS_USB: i32 = 7;
pub(crate) const BUS_SD: i32 = 12;
pub(crate) const BUS_MMC: i32 = 13;
pub(crate) const BUS_NVME: i32 = 17;

/// `BusType` of a `STORAGE_DEVICE_DESCRIPTOR` buffer (offset 28).
pub(crate) fn descriptor_bus_type(buf: &[u8]) -> Option<i32> {
    Some(i32::from_le_bytes(buf.get(28..32)?.try_into().ok()?))
}

/// `IncursSeekPenalty` of a `DEVICE_SEEK_PENALTY_DESCRIPTOR` buffer (offset 8).
pub(crate) fn seek_penalty(buf: &[u8]) -> Option<bool> {
    buf.get(8).map(|b| *b != 0)
}

/// Combines what Windows reports about a drive into a [`DriveKind`].
pub(crate) fn classify_drive(
    bus_type: Option<i32>,
    seek_penalty: Option<bool>,
    removable: bool,
    remote: bool,
    fallback: DriveKind,
) -> DriveKind {
    if remote {
        return DriveKind::Network;
    }
    if removable || matches!(bus_type, Some(BUS_USB | BUS_SD | BUS_MMC)) {
        return DriveKind::Removable;
    }
    if bus_type == Some(BUS_NVME) {
        return DriveKind::Nvme;
    }
    match seek_penalty {
        Some(false) => DriveKind::Ssd,
        Some(true) => DriveKind::Hdd,
        None => fallback,
    }
}

/// HDR is on when the "advanced color enabled" bit (bit 1) of the value is set.
pub(crate) fn advanced_color_enabled(value: u32) -> bool {
    value & 0b10 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_name_fixes_windows_11() {
        assert_eq!(
            windows_os_name("Windows 10 Pro", Some(26200), Some("25H2")),
            "Windows 11 Pro 25H2"
        );
        assert_eq!(
            windows_os_name("Windows 10 Pro", Some(19045), Some("22H2")),
            "Windows 10 Pro 22H2"
        );
        assert_eq!(
            windows_os_name("Windows 11 Home", None, None),
            "Windows 11 Home"
        );
        assert_eq!(
            windows_os_name("Windows 10 Home", Some(22000), Some("")),
            "Windows 11 Home"
        );
    }

    #[test]
    fn driver_dates() {
        assert_eq!(driver_date("3-14-2025").as_deref(), Some("2025-03-14"));
        assert_eq!(driver_date("12-1-2024").as_deref(), Some("2024-12-01"));
        assert_eq!(driver_date("1/2/2023").as_deref(), Some("2023-01-02"));
        assert_eq!(driver_date("nonsense"), None);
        assert_eq!(driver_date("13-1-2024"), None);
        assert_eq!(driver_date("1-2"), None);
    }

    #[test]
    fn refresh_rounding() {
        assert_eq!(refresh_hz(143_981, 1000), 143.98);
        assert_eq!(refresh_hz(60_000, 1001), 59.94);
        assert_eq!(refresh_hz(165, 1), 165.0);
        assert_eq!(refresh_hz(60, 0), 0.0);
    }

    #[test]
    fn decodes_le_values() {
        assert_eq!(le_u64(&[0, 0, 0, 0x80]), Some(0x8000_0000));
        assert_eq!(le_u64(&[0, 0, 0, 0, 3, 0, 0, 0]), Some(3 << 32));
        assert_eq!(le_u64(&[1, 2, 3]), None);
    }

    #[test]
    fn decodes_utf16() {
        let bytes: Vec<u8> = "NVIDIA\0junk"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(utf16_bytes_to_string(&bytes), "NVIDIA");
    }

    #[test]
    fn decodes_storage_descriptors() {
        let mut buf = [0u8; 40];
        buf[28..32].copy_from_slice(&BUS_NVME.to_le_bytes());
        assert_eq!(descriptor_bus_type(&buf), Some(BUS_NVME));
        assert_eq!(descriptor_bus_type(&buf[..10]), None);
        let mut sp = [0u8; 9];
        assert_eq!(seek_penalty(&sp), Some(false));
        sp[8] = 1;
        assert_eq!(seek_penalty(&sp), Some(true));
        assert_eq!(seek_penalty(&sp[..4]), None);
    }

    #[test]
    fn classifies_drives() {
        use DriveKind::*;
        assert_eq!(
            classify_drive(Some(BUS_NVME), Some(false), false, false, Unknown),
            Nvme
        );
        assert_eq!(
            classify_drive(Some(1), Some(false), false, false, Unknown),
            Ssd
        );
        assert_eq!(
            classify_drive(Some(1), Some(true), false, false, Unknown),
            Hdd
        );
        assert_eq!(
            classify_drive(Some(BUS_USB), Some(false), false, false, Unknown),
            Removable
        );
        assert_eq!(classify_drive(None, None, true, false, Hdd), Removable);
        assert_eq!(classify_drive(None, None, false, true, Hdd), Network);
        assert_eq!(classify_drive(None, None, false, false, Ssd), Ssd);
    }

    #[test]
    fn advanced_color_bit() {
        assert!(advanced_color_enabled(0b11));
        assert!(advanced_color_enabled(0b10));
        assert!(!advanced_color_enabled(0b01));
        assert!(!advanced_color_enabled(0));
    }

    #[test]
    fn pseudo_mounts() {
        assert!(is_pseudo("/run/user/1000", "tmpfs"));
        assert!(is_pseudo("/snap/core/1", "squashfs"));
        assert!(is_pseudo("/sys/fs/cgroup", "cgroup2"));
        assert!(!is_pseudo("/", "ext4"));
        assert!(!is_pseudo("C:\\", "NTFS"));
        assert!(!is_pseudo("/home", "btrfs"));
    }

    #[test]
    fn cpu_name_is_collapsed() {
        assert_eq!(
            clean_cpu_name("  12th Gen   Intel(R) Core  "),
            "12th Gen Intel(R) Core"
        );
    }
}
