//! Dev tool: prints the installed Steam games detected on this machine.
//! Run with `cargo run -p questkeep-core --example scan_steam`.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use questkeep_core::providers::LibraryProvider;
use questkeep_core::providers::steam::SteamProvider;

fn main() {
    let steam = SteamProvider::detect();
    println!("Steam roots: {:?}", steam.roots());
    match steam.scan_installed() {
        Ok(games) => {
            for g in &games {
                println!(
                    "{:>8}  {:<40} {}",
                    g.source_game_id,
                    g.name,
                    g.install_dir.as_deref().unwrap_or("-")
                );
            }
            println!("{} games", games.len());
        }
        Err(err) => eprintln!("scan failed: {err}"),
    }
}
