//! Windows-only readers: OS name (registry), GPUs (SetupAPI + driver registry key), displays
//! (display config API) and drive kinds (storage IOCTLs).
//!
//! One of the few modules with `unsafe` code. Every handle and registry key it opens is released
//! by a guard, also on early returns.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DICS_FLAG_GLOBAL, DIGCF_PRESENT, DIREG_DRV, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiOpenDevRegKey,
};
use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::IOCTL_STORAGE_QUERY_PROPERTY;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_BINARY, REG_DWORD, REG_QWORD, REG_SZ, RegCloseKey,
    RegOpenKeyExW, RegQueryValueExW,
};
use windows_sys::core::GUID;

use crate::common;
use crate::{DisplayInfo, DriveInfo, DriveKind, GpuInfo};

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_REMOTE: u32 = 4;

/// `GUID_DEVCLASS_DISPLAY` {4d36e968-e325-11ce-bfc1-08002be10318}.
const GUID_DEVCLASS_DISPLAY: GUID = GUID {
    data1: 0x4d36_e968,
    data2: 0xe325,
    data3: 0x11ce,
    data4: [0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x03, 0x18],
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------- registry

/// An open registry key, closed on drop.
struct Key(HKEY);

impl Key {
    fn open_local_machine(subkey: &str) -> Option<Key> {
        let name = wide(subkey);
        let mut key: HKEY = null_mut();
        // SAFETY: `name` is NUL terminated and outlives the call; `key` is a valid out pointer.
        let rc = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, name.as_ptr(), 0, KEY_READ, &mut key) };
        (rc == ERROR_SUCCESS && !key.is_null()).then_some(Key(key))
    }

    /// The raw type and bytes of a value.
    fn value(&self, name: &str) -> Option<(u32, Vec<u8>)> {
        read_value(self.0, name)
    }

    fn string(&self, name: &str) -> Option<String> {
        value_string(&self.value(name)?)
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by `open_local_machine` and is closed exactly once.
        unsafe { RegCloseKey(self.0) };
    }
}

fn read_value(key: HKEY, name: &str) -> Option<(u32, Vec<u8>)> {
    let name = wide(name);
    let mut kind = 0u32;
    let mut len = 0u32;
    // SAFETY: first call only asks for the type and size; all pointers are valid.
    let rc =
        unsafe { RegQueryValueExW(key, name.as_ptr(), null(), &mut kind, null_mut(), &mut len) };
    if rc != ERROR_SUCCESS || len == 0 || len > 64 * 1024 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` is `len` bytes long, as passed in `len`.
    let rc = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            null(),
            &mut kind,
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    buf.truncate(len as usize);
    Some((kind, buf))
}

fn value_string((kind, bytes): &(u32, Vec<u8>)) -> Option<String> {
    (*kind == REG_SZ)
        .then(|| common::non_empty(&common::utf16_bytes_to_string(bytes)))
        .flatten()
}

// ---------------------------------------------------------------- OS

pub(crate) fn os_name() -> Option<String> {
    let key = Key::open_local_machine(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")?;
    let product = key.string("ProductName")?;
    let build = key
        .string("CurrentBuildNumber")
        .and_then(|b| b.trim().parse::<u32>().ok());
    let display = key
        .string("DisplayVersion")
        .or_else(|| key.string("ReleaseId"));
    Some(common::windows_os_name(&product, build, display.as_deref()))
}

// ---------------------------------------------------------------- GPUs

/// A SetupAPI device info set, destroyed on drop.
struct DevInfoSet(isize);

impl Drop for DevInfoSet {
    fn drop(&mut self) {
        // SAFETY: the set came from `SetupDiGetClassDevsW` and is destroyed once.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

/// A key opened with `SetupDiOpenDevRegKey`, closed on drop.
struct DevKey(HKEY);

impl Drop for DevKey {
    fn drop(&mut self) {
        // SAFETY: closed exactly once.
        unsafe { RegCloseKey(self.0) };
    }
}

fn memory_size(key: HKEY) -> Option<u64> {
    for name in [
        "HardwareInformation.qwMemorySize",
        "HardwareInformation.MemorySize",
    ] {
        if let Some((kind, bytes)) = read_value(key, name)
            && matches!(kind, REG_QWORD | REG_DWORD | REG_BINARY)
            && let Some(size) = common::le_u64(&bytes)
            && size > 0
        {
            return Some(size);
        }
    }
    None
}

/// Present display adapters that report video memory (virtual adapters do not), biggest first.
pub(crate) fn gpus() -> Vec<GpuInfo> {
    // SAFETY: the class GUID is a valid constant; the other arguments may be null.
    let set =
        unsafe { SetupDiGetClassDevsW(&GUID_DEVCLASS_DISPLAY, null(), null_mut(), DIGCF_PRESENT) };
    if set == INVALID_HANDLE_VALUE as isize || set == 0 {
        tracing::debug!("cannot list display devices");
        return Vec::new();
    }
    let set = DevInfoSet(set);
    let mut out = Vec::new();
    for index in 0.. {
        // SAFETY: zeroed SP_DEVINFO_DATA is valid once `cbSize` is set.
        let mut info: SP_DEVINFO_DATA = unsafe { zeroed() };
        info.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
        // SAFETY: `set.0` is a live set and `info` a valid out pointer.
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut info) } == 0 {
            break;
        }
        // SAFETY: arguments follow the documented contract; the key is closed by `DevKey`.
        let raw =
            unsafe { SetupDiOpenDevRegKey(set.0, &info, DICS_FLAG_GLOBAL, 0, DIREG_DRV, KEY_READ) };
        if raw.is_null() || raw as isize == -1 {
            continue;
        }
        let key = DevKey(raw);
        let Some(memory) = memory_size(key.0) else {
            continue;
        };
        let text = |name: &str| read_value(key.0, name).and_then(|v| value_string(&v));
        let Some(name) = text("DriverDesc") else {
            continue;
        };
        out.push(GpuInfo {
            name,
            memory_bytes: Some(memory),
            driver_version: text("DriverVersion"),
            driver_date: text("DriverDate").and_then(|d| common::driver_date(&d)),
        });
    }
    out.sort_by_key(|g| std::cmp::Reverse(g.memory_bytes));
    out
}

// ---------------------------------------------------------------- displays

pub(crate) fn displays() -> Vec<DisplayInfo> {
    let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = Vec::new();
    let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = Vec::new();
    let mut ok = false;
    for _ in 0..3 {
        let (mut np, mut nm) = (0u32, 0u32);
        // SAFETY: valid out pointers.
        if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) }
            != ERROR_SUCCESS
        {
            break;
        }
        // SAFETY: the structs are plain C data; all-zero is a valid value.
        paths = (0..np).map(|_| unsafe { zeroed() }).collect();
        modes = (0..nm).map(|_| unsafe { zeroed() }).collect();
        // SAFETY: the buffers hold `np` and `nm` elements, as passed.
        let rc = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut np,
                paths.as_mut_ptr(),
                &mut nm,
                modes.as_mut_ptr(),
                null_mut(),
            )
        };
        if rc == ERROR_SUCCESS {
            paths.truncate(np as usize);
            modes.truncate(nm as usize);
            ok = true;
            break;
        }
        if rc != ERROR_INSUFFICIENT_BUFFER {
            break;
        }
    }
    if !ok {
        tracing::debug!("cannot query the display configuration");
        return Vec::new();
    }

    let mut seen = Vec::new();
    let mut out = Vec::new();
    for path in &paths {
        let target = &path.targetInfo;
        let id = (
            target.adapterId.LowPart,
            target.adapterId.HighPart,
            target.id,
        );
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);

        // SAFETY: without the virtual-mode flag the index is a plain index (union read).
        let mode_idx = unsafe { path.sourceInfo.Anonymous.modeInfoIdx } as usize;
        let Some(mode) = modes.get(mode_idx) else {
            continue;
        };
        if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
            continue;
        }
        // SAFETY: `infoType` says the union holds a source mode.
        let source = unsafe { mode.Anonymous.sourceMode };

        out.push(DisplayInfo {
            name: friendly_name(path).unwrap_or_else(|| format!("Display {}", out.len() + 1)),
            width: source.width,
            height: source.height,
            refresh_hz: common::refresh_hz(
                target.refreshRate.Numerator,
                target.refreshRate.Denominator,
            ),
            hdr: hdr_enabled(path),
            primary: source.position.x == 0 && source.position.y == 0,
        });
    }
    out
}

fn friendly_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    // SAFETY: zeroed is valid; the header is filled in before the call.
    let mut name: DISPLAYCONFIG_TARGET_DEVICE_NAME = unsafe { zeroed() };
    name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
    name.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
    name.header.adapterId = path.targetInfo.adapterId;
    name.header.id = path.targetInfo.id;
    // SAFETY: the header describes a buffer of exactly this struct.
    if unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } != ERROR_SUCCESS as i32 {
        return None;
    }
    let units = &name.monitorFriendlyDeviceName;
    let len = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    common::non_empty(&String::from_utf16_lossy(&units[..len]))
}

fn hdr_enabled(path: &DISPLAYCONFIG_PATH_INFO) -> Option<bool> {
    // SAFETY: zeroed is valid; the header is filled in before the call.
    let mut info: DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO = unsafe { zeroed() };
    info.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO;
    info.header.size = size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32;
    info.header.adapterId = path.targetInfo.adapterId;
    info.header.id = path.targetInfo.id;
    // SAFETY: the header describes a buffer of exactly this struct.
    if unsafe { DisplayConfigGetDeviceInfo(&mut info.header) } != ERROR_SUCCESS as i32 {
        return None;
    }
    // SAFETY: both union members are a u32.
    Some(common::advanced_color_enabled(unsafe {
        info.Anonymous.value
    }))
}

// ---------------------------------------------------------------- drives

/// A Win32 handle closed on drop.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a valid handle from `CreateFileW`, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Refines the kind of each drive with the storage stack's answers; keeps sysinfo's guess when
/// the device cannot be queried.
pub(crate) fn refine_drives(drives: &mut [DriveInfo]) {
    for drive in drives {
        let Some(letter) = drive.mount.chars().next().filter(char::is_ascii_alphabetic) else {
            continue;
        };
        let root = wide(&format!("{letter}:\\"));
        // SAFETY: NUL terminated string.
        let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
        let (bus, penalty) = storage_properties(letter);
        drive.kind = common::classify_drive(
            bus,
            penalty,
            drive.kind == DriveKind::Removable || drive_type == DRIVE_REMOVABLE,
            drive_type == DRIVE_REMOTE,
            drive.kind,
        );
    }
}

fn open_volume(letter: char) -> Option<Handle> {
    let path = wide(&format!(r"\\.\{letter}:"));
    // SAFETY: NUL terminated path; access 0 only allows querying properties.
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    (h != INVALID_HANDLE_VALUE && !h.is_null()).then_some(Handle(h))
}

/// `STORAGE_PROPERTY_QUERY` bytes: property id, `PropertyStandardQuery`, one parameter byte.
fn query_property(handle: &Handle, property_id: u32, out: &mut [u8]) -> bool {
    let mut query = [0u8; 12];
    query[..4].copy_from_slice(&property_id.to_le_bytes());
    let mut returned = 0u32;
    // SAFETY: input and output buffers are valid for the sizes passed.
    let ok = unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_STORAGE_QUERY_PROPERTY,
            query.as_ptr().cast::<c_void>(),
            query.len() as u32,
            out.as_mut_ptr().cast::<c_void>(),
            out.len() as u32,
            &mut returned,
            null_mut(),
        )
    };
    ok != 0
}

/// Bus type and seek penalty of the disk behind a drive letter.
fn storage_properties(letter: char) -> (Option<i32>, Option<bool>) {
    let Some(handle) = open_volume(letter) else {
        tracing::debug!("cannot open volume {letter}:");
        return (None, None);
    };
    const STORAGE_DEVICE_PROPERTY: u32 = 0;
    const STORAGE_DEVICE_SEEK_PENALTY_PROPERTY: u32 = 7;
    let mut device = [0u8; 1024];
    let bus = query_property(&handle, STORAGE_DEVICE_PROPERTY, &mut device)
        .then(|| common::descriptor_bus_type(&device))
        .flatten();
    let mut seek = [0u8; 16];
    let penalty = query_property(&handle, STORAGE_DEVICE_SEEK_PENALTY_PROPERTY, &mut seek)
        .then(|| common::seek_penalty(&seek))
        .flatten();
    (bus, penalty)
}
