//! Dev tool: prints the Meta Horizon Link apps installed and owned on this machine.
//! Run with `cargo run -p questkeep-store-meta --example scan_meta`.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use questkeep_store_api::LibraryProvider;
use questkeep_store_meta::MetaProvider;

fn main() {
    let meta = MetaProvider::detect();
    let paths = meta.paths();
    println!("Libraries: {:?}", paths.libraries);
    println!("Install folder: {:?}", paths.install_dir);
    println!("Launcher: {:?}", meta.launcher().map(|a| a.target));
    match meta.scan_installed() {
        Ok(games) => {
            for g in &games {
                let action = g.play_action.as_ref();
                println!(
                    "{:>18}  {:<36} {} {}",
                    g.source_game_id,
                    g.name,
                    action.map_or("-", |a| a.target.as_str()),
                    action.and_then(|a| a.args.as_deref()).unwrap_or("")
                );
            }
            println!("{} installed", games.len());
        }
        Err(err) => eprintln!("installed scan failed: {err}"),
    }
    match meta.scan_owned(&serde_json::Value::Null) {
        Ok(games) => {
            for g in &games {
                let covers = meta.cover_candidates(&g.source_game_id).len();
                println!(
                    "{:>18}  {:<36} app={:?} playtime={:?} covers={covers}",
                    g.source_game_id, g.name, g.app, g.playtime_seconds
                );
            }
            println!("{} owned", games.len());
        }
        Err(err) => eprintln!("owned scan failed: {err}"),
    }
}
