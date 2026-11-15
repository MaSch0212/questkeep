//! Reads the target, arguments and working directory of a Windows shortcut (`.lnk`), following
//! the Shell Link Binary File Format ([MS-SHLLINK]). Only what the program picker needs; no COM.

/// What a shortcut starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shortcut {
    /// Absolute path of the target, when the shortcut stores one (advertised MSI shortcuts and
    /// shell items such as Control Panel entries don't).
    pub target: Option<String>,
    /// The target relative to the shortcut's folder (used when there is no absolute one).
    pub relative_path: Option<String>,
    pub working_dir: Option<String>,
    pub args: Option<String>,
}

const HEADER_SIZE: usize = 0x4C;
const HAS_ID_LIST: u32 = 0x1;
const HAS_LINK_INFO: u32 = 0x2;
const HAS_NAME: u32 = 0x4;
const HAS_RELATIVE_PATH: u32 = 0x8;
const HAS_WORKING_DIR: u32 = 0x10;
const HAS_ARGUMENTS: u32 = 0x20;
const IS_UNICODE: u32 = 0x80;
/// LinkInfo flag: the target is on a local volume and `LocalBasePath` is set.
const VOLUME_ID_AND_LOCAL_BASE_PATH: u32 = 0x1;
/// The shell link CLSID {00021401-0000-0000-C000-000000000046} as stored.
const CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// A NUL-terminated string in the system code page; only ASCII is read reliably, other bytes
/// are taken as Latin-1.
fn ansi_at(data: &[u8], at: usize) -> Option<String> {
    let bytes = data.get(at..)?;
    let end = bytes.iter().position(|b| *b == 0)?;
    Some(bytes[..end].iter().map(|b| char::from(*b)).collect())
}

fn utf16_at(data: &[u8], at: usize) -> Option<String> {
    let mut units = Vec::new();
    let mut pos = at;
    loop {
        let unit = u16_at(data, pos)?;
        if unit == 0 {
            break;
        }
        units.push(unit);
        pos += 2;
    }
    String::from_utf16(&units).ok()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Parses a shortcut file's bytes; `None` when they are not a shell link.
pub fn parse(data: &[u8]) -> Option<Shortcut> {
    if u32_at(data, 0)? as usize != HEADER_SIZE || data.get(4..20)? != CLSID {
        return None;
    }
    let flags = u32_at(data, 0x14)?;
    let mut pos = HEADER_SIZE;
    if flags & HAS_ID_LIST != 0 {
        pos += 2 + usize::from(u16_at(data, pos)?);
    }
    let mut target = None;
    if flags & HAS_LINK_INFO != 0 {
        let info = data.get(pos..)?;
        let size = u32_at(info, 0)? as usize;
        let header_size = u32_at(info, 4)? as usize;
        let info_flags = u32_at(info, 8)?;
        if info_flags & VOLUME_ID_AND_LOCAL_BASE_PATH != 0 {
            let suffix_at = u32_at(info, 0x18)? as usize;
            let unicode = header_size >= 0x24;
            let base_unicode = if unicode { u32_at(info, 0x1C)? } else { 0 } as usize;
            let suffix_unicode = if unicode { u32_at(info, 0x20)? } else { 0 } as usize;
            let base = if base_unicode != 0 {
                utf16_at(info, base_unicode)
            } else {
                ansi_at(info, u32_at(info, 0x10)? as usize)
            };
            let suffix = if suffix_unicode != 0 {
                utf16_at(info, suffix_unicode)
            } else if suffix_at != 0 {
                ansi_at(info, suffix_at)
            } else {
                None
            };
            target = base.map(|b| b + suffix.as_deref().unwrap_or_default());
        }
        pos += size;
    }
    let unicode = flags & IS_UNICODE != 0;
    let mut read_string = |present: bool| -> Option<Option<String>> {
        if !present {
            return Some(None);
        }
        let count = usize::from(u16_at(data, pos)?);
        pos += 2;
        let text = if unicode {
            let bytes = data.get(pos..pos + count * 2)?;
            pos += count * 2;
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            let bytes = data.get(pos..pos + count)?;
            pos += count;
            bytes.iter().map(|b| char::from(*b)).collect()
        };
        Some(Some(text))
    };
    let _name = read_string(flags & HAS_NAME != 0)?;
    let relative_path = read_string(flags & HAS_RELATIVE_PATH != 0)?;
    let working_dir = read_string(flags & HAS_WORKING_DIR != 0)?;
    let args = read_string(flags & HAS_ARGUMENTS != 0)?;
    Some(Shortcut {
        target: non_empty(target),
        relative_path: non_empty(relative_path),
        working_dir: non_empty(working_dir),
        args: non_empty(args),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    fn counted(text: &str) -> Vec<u8> {
        let mut out = (text.encode_utf16().count() as u16).to_le_bytes().to_vec();
        out.extend(utf16(text));
        out
    }

    /// A shortcut as Windows writes one: an (opaque) id list, link info with a local base path,
    /// then working directory and arguments as Unicode strings.
    pub(crate) fn shortcut_bytes(target: &str, working_dir: &str, args: &str) -> Vec<u8> {
        let flags = HAS_ID_LIST | HAS_LINK_INFO | HAS_WORKING_DIR | HAS_ARGUMENTS | IS_UNICODE;
        let mut data = vec![0u8; HEADER_SIZE];
        data[0..4].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        data[4..20].copy_from_slice(&CLSID);
        data[0x14..0x18].copy_from_slice(&flags.to_le_bytes());
        // Id list: 4 bytes of anything.
        data.extend(4u16.to_le_bytes());
        data.extend([1, 2, 3, 4]);
        // Link info with an ANSI base path, an empty suffix and a Unicode base path.
        let header = 0x24u32;
        let ansi_at = header;
        let ansi: Vec<u8> = target.bytes().chain([0]).collect();
        let suffix_at = ansi_at + ansi.len() as u32;
        let unicode_at = suffix_at + 1;
        let unicode: Vec<u8> = utf16(target).into_iter().chain([0, 0]).collect();
        let size = unicode_at + unicode.len() as u32;
        for value in [
            size,
            header,
            VOLUME_ID_AND_LOCAL_BASE_PATH,
            0, // volume id (not read)
            ansi_at,
            0,
            suffix_at,
            unicode_at,
            0,
        ] {
            data.extend(value.to_le_bytes());
        }
        data.extend(ansi);
        data.push(0);
        data.extend(unicode);
        data.extend(counted(working_dir));
        data.extend(counted(args));
        data
    }

    #[test]
    fn reads_target_working_dir_and_arguments() {
        let bytes = shortcut_bytes(r"C:\Games\Celeste\Celeste.exe", r"C:\Games\Celeste", "-fs");
        let shortcut = parse(&bytes).unwrap();
        assert_eq!(
            shortcut.target.as_deref(),
            Some(r"C:\Games\Celeste\Celeste.exe")
        );
        assert_eq!(shortcut.working_dir.as_deref(), Some(r"C:\Games\Celeste"));
        assert_eq!(shortcut.args.as_deref(), Some("-fs"));
        assert_eq!(shortcut.relative_path, None);
    }

    #[test]
    fn rejects_other_files_and_survives_truncation() {
        assert_eq!(parse(b"not a shortcut"), None);
        let bytes = shortcut_bytes(r"C:\a.exe", "", "");
        for cut in [10, HEADER_SIZE, HEADER_SIZE + 8, bytes.len() - 3] {
            assert_eq!(parse(&bytes[..cut]), None, "cut at {cut}");
        }
        // Empty strings count as missing.
        let shortcut = parse(&bytes).unwrap();
        assert_eq!((shortcut.working_dir, shortcut.args), (None, None));
    }
}
