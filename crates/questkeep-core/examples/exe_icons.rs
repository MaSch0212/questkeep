//! Dev tool: extracts the icon of each given executable the way the tray icon fetcher does and
//! writes `<out dir>/<file stem>.png` (32x32), to check icon quality on real games. Read-only for
//! the executables.
//!
//! ```text
//! cargo run -p questkeep-core --example exe_icons -- <out dir> <exe>...
//! ```
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use questkeep_core::images::{SquareFrom, icon_png_from_rgba};
use questkeep_core::platform::executable_icon;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(out) = args.next().map(PathBuf::from) else {
        eprintln!("usage: exe_icons <out dir> <exe>...");
        std::process::exit(2);
    };
    if let Err(err) = std::fs::create_dir_all(&out) {
        eprintln!("cannot create {}: {err}", out.display());
        std::process::exit(1);
    }
    for exe in args.map(PathBuf::from) {
        let name = exe
            .file_stem()
            .map_or_else(|| "icon".into(), |s| s.to_string_lossy().into_owned());
        let result = executable_icon(&exe, 48).ok_or("no icon").and_then(|icon| {
            let size = (icon.width, icon.height);
            icon_png_from_rgba(&icon, SquareFrom::Icon)
                .map(|png| (png, size))
                .ok_or("not convertible")
        });
        match result {
            Ok((png, (w, h))) => {
                let target = out.join(format!("{name}.png"));
                match std::fs::write(&target, png) {
                    Ok(()) => println!("{name}: extracted {w}x{h} -> {}", target.display()),
                    Err(err) => println!("{name}: write failed: {err}"),
                }
            }
            Err(reason) => println!("{name}: {reason}"),
        }
    }
}
