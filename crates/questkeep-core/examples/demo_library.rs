//! Fills a data folder with a made-up library for screenshots: games in several stores with
//! metadata, a few years of play sessions, statuses, scores, categories and combined copies.
//! Nothing in it comes from a real library. Artwork is drawn afterwards by the page (see
//! `docs/screenshots.md`), from the list this writes to `<data dir>/demo-art.json`.
//!
//! cargo run -p questkeep-core --example demo_library -- <empty data dir>
#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc, Weekday};
use questkeep_core::db::Database;
use questkeep_core::model::{
    CompletionSettings, CompletionStatus, GameLink, GameMetadata, GameQuery, HiddenFilter,
    ImportedGame, KindFilter, MetadataPriority, PlayAction, Score, TimeToBeat,
};
use uuid::Uuid;

/// One made-up game. `stores` lists where the user owns it (the first one is preferred).
struct Demo {
    name: &'static str,
    stores: &'static [&'static str],
    developer: &'static str,
    publisher: &'static str,
    year: i32,
    genres: &'static [&'static str],
    tags: &'static [&'static str],
    features: &'static [&'static str],
    critic: Option<u8>,
    community: Option<u8>,
    /// Hours: main story, main + extras, completionist.
    ttb: Option<(u64, u64, u64)>,
    size_gb: f64,
    installed: bool,
    /// Hours played, roughly.
    hours: f64,
    /// Days since the last session (`None`: never played).
    last_days: Option<i64>,
    status: &'static str,
    user_score: Option<u8>,
    favorite: bool,
    categories: &'static [&'static str],
    description: &'static str,
    /// Hue of the generated artwork.
    hue: u16,
}

const SP: &[&str] = &[
    "Single-player",
    "Achievements",
    "Controller support",
    "Cloud saves",
];
const COOP: &[&str] = &[
    "Single-player",
    "Online Co-op",
    "Local Co-op",
    "Achievements",
    "Controller support",
];
const MP: &[&str] = &[
    "Multiplayer",
    "Online PvP",
    "Cross-platform",
    "Achievements",
];

#[rustfmt::skip]
const GAMES: &[Demo] = &[
    Demo { name: "Starfall Tactics", stores: &["steam"], developer: "Lanternfish Games", publisher: "Lanternfish Games", year: 2024, genres: &["Strategy", "Tactics"], tags: &["Turn-based", "Sci-fi", "Squad"], features: SP, critic: Some(88), community: Some(91), ttb: Some((34, 52, 90)), size_gb: 18.4, installed: true, hours: 212.0, last_days: Some(0), status: "playing", user_score: Some(92), favorite: true, categories: &["Weekend picks"], description: "Command a ragtag fleet across a collapsing star cluster. Every jump costs fuel, every battle costs crew, and every choice changes which systems are still standing when the war ends.", hue: 220 },
    Demo { name: "Moonlit Harbor", stores: &["gog"], developer: "Quiet Tide Studio", publisher: "Quiet Tide Studio", year: 2023, genres: &["Adventure", "Simulation"], tags: &["Cozy", "Fishing", "Relaxing"], features: SP, critic: Some(84), community: Some(93), ttb: Some((18, 30, 55)), size_gb: 4.2, installed: true, hours: 96.0, last_days: Some(1), status: "playing", user_score: Some(88), favorite: true, categories: &["Cozy evenings"], description: "Run a small harbor on a sleepy island, catch fish under the full moon and help the townsfolk rebuild the lighthouse, one season at a time.", hue: 200 },
    Demo { name: "Iron Meridian", stores: &["steam", "epic"], developer: "Northgate Interactive", publisher: "Halcyon Publishing", year: 2022, genres: &["Action", "RPG"], tags: &["Open World", "Mechs", "Story Rich"], features: SP, critic: Some(86), community: Some(82), ttb: Some((42, 70, 120)), size_gb: 96.5, installed: true, hours: 164.0, last_days: Some(3), status: "beaten", user_score: Some(85), favorite: false, categories: &[], description: "Pilot a salvaged war machine through a frontier where the old empire's factories still hum. Upgrade your frame, choose your allies and decide who controls the meridian.", hue: 18 },
    Demo { name: "Pixel Pantry", stores: &["steam"], developer: "Crumb & Co.", publisher: "Crumb & Co.", year: 2021, genres: &["Simulation", "Casual"], tags: &["Cooking", "Pixel Graphics", "Co-op"], features: COOP, critic: Some(80), community: Some(89), ttb: Some((12, 20, 35)), size_gb: 1.1, installed: true, hours: 58.0, last_days: Some(2), status: "played", user_score: Some(80), favorite: false, categories: &["Couch co-op", "Cozy evenings"], description: "A frantic little restaurant where the orders never stop and the kitchen floor keeps rearranging itself. Better with friends on the couch.", hue: 35 },
    Demo { name: "Echoes of Verdant", stores: &["epic"], developer: "Willowlight", publisher: "Willowlight", year: 2025, genres: &["Adventure", "Puzzle"], tags: &["Atmospheric", "Exploration", "Hand-drawn"], features: SP, critic: Some(90), community: Some(87), ttb: Some((9, 12, 16)), size_gb: 7.8, installed: true, hours: 14.5, last_days: Some(9), status: "beaten", user_score: Some(94), favorite: true, categories: &["Weekend picks"], description: "Wander an overgrown city that remembers its people through sound. Listen, retrace, and wake the garden that swallowed the streets.", hue: 140 },
    Demo { name: "Rally Zenith", stores: &["xbox"], developer: "Apex Line Studios", publisher: "Apex Line Studios", year: 2025, genres: &["Racing", "Sport"], tags: &["Driving", "Open World", "Multiplayer"], features: MP, critic: Some(89), community: Some(84), ttb: Some((20, 45, 110)), size_gb: 128.0, installed: true, hours: 141.0, last_days: Some(0), status: "playing", user_score: Some(87), favorite: true, categories: &["Couch co-op"], description: "Race across a coastline of cliffs, salt flats and mountain passes. Build your garage, join seasonal festivals and chase the perfect line.", hue: 5 },
    Demo { name: "Hollow Crown", stores: &["steam"], developer: "Blackthorn Works", publisher: "Blackthorn Works", year: 2020, genres: &["Action", "RPG"], tags: &["Souls-like", "Dark Fantasy", "Difficult"], features: SP, critic: Some(91), community: Some(90), ttb: Some((38, 55, 95)), size_gb: 32.0, installed: false, hours: 187.0, last_days: Some(160), status: "completed", user_score: Some(95), favorite: true, categories: &[], description: "A fallen kingdom, a crown that whispers, and bosses that remember how you died last time. Patience is the only armor that never breaks.", hue: 275 },
    Demo { name: "Clockwork Garden", stores: &["gog"], developer: "Tinker Lane", publisher: "Tinker Lane", year: 2019, genres: &["Puzzle"], tags: &["Logic", "Relaxing", "Steampunk"], features: &["Single-player"], critic: Some(83), community: Some(88), ttb: Some((10, 14, 22)), size_gb: 0.9, installed: true, hours: 22.0, last_days: Some(40), status: "beaten", user_score: Some(82), favorite: false, categories: &["Cozy evenings"], description: "Wind springs, route gears and coax brass flowers into bloom across a hundred hand-built puzzles.", hue: 95 },
    Demo { name: "Frostline Outpost", stores: &["steam"], developer: "Polar Bit", publisher: "Polar Bit", year: 2023, genres: &["Strategy", "Simulation"], tags: &["Survival", "City Builder", "Base Building"], features: SP, critic: Some(85), community: Some(86), ttb: Some((25, 50, 100)), size_gb: 9.6, installed: true, hours: 119.0, last_days: Some(5), status: "playing", user_score: Some(86), favorite: false, categories: &["Backlog"], description: "Keep a research outpost alive through an endless winter. Heat, food and hope are all running low, and the storm is still coming.", hue: 190 },
    Demo { name: "Neon Drift 2088", stores: &["epic"], developer: "Synthwave Labs", publisher: "Synthwave Labs", year: 2022, genres: &["Racing", "Arcade"], tags: &["Cyberpunk", "Fast-Paced", "Soundtrack"], features: MP, critic: Some(78), community: Some(81), ttb: Some((6, 12, 30)), size_gb: 12.3, installed: false, hours: 31.0, last_days: Some(210), status: "played", user_score: Some(74), favorite: false, categories: &[], description: "Neon-soaked street racing at speeds that bend the skyline. Drift, boost and beat the city's ghost times.", hue: 310 },
    Demo { name: "The Last Lantern", stores: &["steam", "gog"], developer: "Ember Hollow", publisher: "Ember Hollow", year: 2021, genres: &["Adventure", "Horror"], tags: &["Atmospheric", "Story Rich", "Psychological"], features: SP, critic: Some(87), community: Some(85), ttb: Some((7, 9, 12)), size_gb: 14.0, installed: false, hours: 9.0, last_days: Some(320), status: "beaten", user_score: Some(89), favorite: false, categories: &[], description: "A night in a lighthouse keeper's house where the lantern must never go out, and something outside wants it to.", hue: 40 },
    Demo { name: "Skyward Kingdoms", stores: &["ubisoft"], developer: "Aerie Studio", publisher: "Grand Arc Entertainment", year: 2024, genres: &["Strategy"], tags: &["City Builder", "Fantasy", "Floating Islands"], features: SP, critic: Some(82), community: Some(79), ttb: Some((30, 60, 140)), size_gb: 41.0, installed: true, hours: 74.0, last_days: Some(14), status: "playing", user_score: Some(78), favorite: false, categories: &["Backlog"], description: "Build cities on islands that drift above the clouds, connect them with bridges and keep the winds on your side.", hue: 175 },
    Demo { name: "Gridlock Legends", stores: &["ea"], developer: "Fourth Quarter", publisher: "Fourth Quarter", year: 2025, genres: &["Sport"], tags: &["Football", "Multiplayer", "Competitive"], features: MP, critic: Some(76), community: Some(68), ttb: None, size_gb: 52.0, installed: true, hours: 66.0, last_days: Some(4), status: "played", user_score: Some(70), favorite: false, categories: &["Couch co-op"], description: "Arcade football with oversized personalities, trick plays and a season mode that never quite ends.", hue: 120 },
    Demo { name: "Runebound Tavern", stores: &["steam"], developer: "Copper Mug Games", publisher: "Copper Mug Games", year: 2024, genres: &["Simulation", "RPG"], tags: &["Management", "Fantasy", "Cozy"], features: SP, critic: Some(81), community: Some(90), ttb: Some((20, 35, 60)), size_gb: 3.4, installed: true, hours: 47.0, last_days: Some(6), status: "playing", user_score: Some(84), favorite: false, categories: &["Cozy evenings"], description: "Run the inn where adventurers rest between quests. Brew, hire, gossip, and send the right heroes to the right trouble.", hue: 28 },
    Demo { name: "Abyssal Signal", stores: &["gog"], developer: "Deepwater Collective", publisher: "Deepwater Collective", year: 2022, genres: &["Adventure", "Horror"], tags: &["Underwater", "Sci-fi", "Exploration"], features: SP, critic: Some(84), community: Some(86), ttb: Some((11, 15, 20)), size_gb: 22.0, installed: false, hours: 0.0, last_days: None, status: "not-played", user_score: None, favorite: false, categories: &["Backlog"], description: "A distress call from the bottom of the ocean, a submarine that creaks too much, and a crew that stopped answering.", hue: 210 },
    Demo { name: "Paper Knights", stores: &["steam"], developer: "Foldline", publisher: "Foldline", year: 2018, genres: &["Platformer", "Action"], tags: &["Co-op", "Papercraft", "Family Friendly"], features: COOP, critic: Some(79), community: Some(88), ttb: Some((8, 12, 18)), size_gb: 2.6, installed: true, hours: 21.0, last_days: Some(70), status: "beaten", user_score: Some(81), favorite: false, categories: &["Couch co-op"], description: "Cut, fold and jump through a storybook castle with up to four friends.", hue: 50 },
    Demo { name: "Dustwind Frontier", stores: &["epic"], developer: "Red Mesa", publisher: "Halcyon Publishing", year: 2023, genres: &["Action", "Shooter"], tags: &["Western", "Open World", "Story Rich"], features: SP, critic: Some(83), community: Some(80), ttb: Some((28, 45, 80)), size_gb: 88.0, installed: false, hours: 0.0, last_days: None, status: "not-played", user_score: None, favorite: false, categories: &["Backlog"], description: "A lawless valley, a stolen railway and a sheriff's badge you never asked for.", hue: 25 },
    Demo { name: "Lumen Protocol", stores: &["steam"], developer: "Halfbit", publisher: "Halfbit", year: 2025, genres: &["Puzzle", "Action"], tags: &["First-Person", "Physics", "Sci-fi"], features: SP, critic: Some(92), community: Some(94), ttb: Some((10, 14, 18)), size_gb: 11.0, installed: true, hours: 13.0, last_days: Some(20), status: "beaten", user_score: Some(96), favorite: true, categories: &["Weekend picks"], description: "Redirect light through a testing facility that keeps rewriting its own rules.", hue: 260 },
    Demo { name: "Harvest Hollow", stores: &["steam"], developer: "Little Acre", publisher: "Little Acre", year: 2020, genres: &["Simulation", "RPG"], tags: &["Farming", "Cozy", "Pixel Graphics"], features: COOP, critic: Some(85), community: Some(95), ttb: Some((45, 90, 160)), size_gb: 0.7, installed: true, hours: 228.0, last_days: Some(30), status: "played", user_score: Some(91), favorite: true, categories: &["Cozy evenings", "Couch co-op"], description: "Inherit a run-down farm, befriend the valley and discover what the old well is hiding.", hue: 85 },
    Demo { name: "Vector Siege", stores: &["battlenet"], developer: "Arclight Entertainment", publisher: "Arclight Entertainment", year: 2021, genres: &["Strategy"], tags: &["RTS", "Competitive", "Sci-fi"], features: MP, critic: Some(88), community: Some(83), ttb: Some((20, 30, 60)), size_gb: 34.0, installed: true, hours: 302.0, last_days: Some(12), status: "played", user_score: Some(83), favorite: false, categories: &[], description: "Three factions, one contested moon. Build, scout and outthink your opponent in fast real-time battles.", hue: 230 },
    Demo { name: "Cinder & Sage", stores: &["gog"], developer: "Two Candles", publisher: "Two Candles", year: 2024, genres: &["RPG"], tags: &["Turn-based", "Party-based", "Fantasy"], features: SP, critic: Some(89), community: Some(91), ttb: Some((55, 85, 140)), size_gb: 38.0, installed: true, hours: 61.0, last_days: Some(8), status: "playing", user_score: Some(90), favorite: false, categories: &["Weekend picks"], description: "A party of misfits, a world on fire and a dozen ways to talk your way out of every battle.", hue: 15 },
    Demo { name: "Tidebreaker", stores: &["amazon"], developer: "Coastline Studio", publisher: "Coastline Studio", year: 2022, genres: &["Action", "Adventure"], tags: &["Pirates", "Sailing", "Open World"], features: SP, critic: Some(77), community: Some(80), ttb: Some((22, 35, 60)), size_gb: 46.0, installed: false, hours: 18.0, last_days: Some(250), status: "played", user_score: None, favorite: false, categories: &[], description: "Captain a stolen ship across a stormy archipelago and outrun the navy that wants it back.", hue: 195 },
    Demo { name: "Glimmerwood", stores: &["steam"], developer: "Fernlight", publisher: "Fernlight", year: 2025, genres: &["Platformer"], tags: &["Metroidvania", "Hand-drawn", "Atmospheric"], features: SP, critic: Some(90), community: Some(92), ttb: Some((16, 24, 35)), size_gb: 6.5, installed: true, hours: 26.0, last_days: Some(1), status: "playing", user_score: Some(91), favorite: false, categories: &[], description: "Explore a forest of glowing roots, learn new ways to move and uncover why the light is fading.", hue: 160 },
    Demo { name: "Orbital Freight Co.", stores: &["steam"], developer: "Payload Games", publisher: "Payload Games", year: 2023, genres: &["Simulation"], tags: &["Space", "Logistics", "Sandbox"], features: SP, critic: Some(80), community: Some(84), ttb: None, size_gb: 5.1, installed: true, hours: 83.0, last_days: Some(18), status: "played", user_score: Some(79), favorite: false, categories: &[], description: "Haul cargo between space stations, juggle contracts and keep an aging fleet in one piece.", hue: 245 },
    Demo { name: "Bramble Bounce", stores: &["epic"], developer: "Hopscotch", publisher: "Hopscotch", year: 2019, genres: &["Platformer", "Casual"], tags: &["Family Friendly", "Colorful"], features: COOP, critic: Some(74), community: Some(82), ttb: Some((5, 8, 12)), size_gb: 1.9, installed: false, hours: 0.0, last_days: None, status: "not-played", user_score: None, favorite: false, categories: &[], description: "A springy little hedgehog bounces through gardens full of secrets.", hue: 330 },
    Demo { name: "Sentinel Spire", stores: &["steam"], developer: "Keystone Games", publisher: "Keystone Games", year: 2022, genres: &["Strategy"], tags: &["Tower Defense", "Roguelite"], features: SP, critic: Some(82), community: Some(87), ttb: Some((15, 25, 50)), size_gb: 2.2, installed: true, hours: 39.0, last_days: Some(45), status: "played", user_score: Some(77), favorite: false, categories: &[], description: "Raise a tower one floor at a time and hold it against waves that learn from your defenses.", hue: 0 },
    Demo { name: "Murmur Lake", stores: &["gog"], developer: "Stillwater", publisher: "Stillwater", year: 2017, genres: &["Adventure"], tags: &["Mystery", "Narrative", "Walking Simulator"], features: &["Single-player"], critic: Some(81), community: Some(83), ttb: Some((4, 5, 6)), size_gb: 3.0, installed: false, hours: 5.0, last_days: Some(400), status: "beaten", user_score: Some(80), favorite: false, categories: &[], description: "A summer cabin, a missing sister and a lake that hums at night.", hue: 185 },
    Demo { name: "Kitebound", stores: &["xbox"], developer: "Breeze Works", publisher: "Breeze Works", year: 2024, genres: &["Adventure", "Casual"], tags: &["Flying", "Relaxing", "Exploration"], features: SP, critic: Some(84), community: Some(86), ttb: Some((6, 9, 14)), size_gb: 8.0, installed: true, hours: 7.5, last_days: Some(25), status: "beaten", user_score: Some(83), favorite: false, categories: &["Cozy evenings"], description: "Ride the wind on a patchwork kite and deliver letters to the villages of a quiet valley.", hue: 45 },
    Demo { name: "Steel Requiem", stores: &["ea"], developer: "Monolith Forge", publisher: "Fourth Quarter", year: 2023, genres: &["Shooter", "Action"], tags: &["Multiplayer", "Military", "Competitive"], features: MP, critic: Some(79), community: Some(71), ttb: Some((8, 12, 30)), size_gb: 110.0, installed: false, hours: 54.0, last_days: Some(130), status: "abandoned", user_score: Some(62), favorite: false, categories: &[], description: "Large-scale battles with destructible cities, vehicles and squads of up to five.", hue: 210 },
    Demo { name: "Quiet Quarry", stores: &["steam"], developer: "Stonefold", publisher: "Stonefold", year: 2025, genres: &["Simulation", "Puzzle"], tags: &["Relaxing", "Building"], features: SP, critic: None, community: Some(88), ttb: None, size_gb: 0.6, installed: true, hours: 3.0, last_days: Some(2), status: "playing", user_score: None, favorite: false, categories: &["Cozy evenings"], description: "Carve terraces into a hillside, one stone at a time, and watch a village grow around them.", hue: 30 },
    Demo { name: "Astral Courier", stores: &["steam"], developer: "Lanternfish Games", publisher: "Lanternfish Games", year: 2021, genres: &["Action", "Adventure"], tags: &["Space", "Delivery", "Story Rich"], features: SP, critic: Some(81), community: Some(84), ttb: Some((14, 20, 32)), size_gb: 9.2, installed: false, hours: 24.0, last_days: Some(500), status: "beaten", user_score: Some(80), favorite: false, categories: &[], description: "Deliver packages across a solar system that is slowly falling apart, and read the letters you were told not to open.", hue: 265 },
    Demo { name: "Whisker Detectives", stores: &["epic", "gog"], developer: "Paw Print", publisher: "Paw Print", year: 2024, genres: &["Adventure", "Puzzle"], tags: &["Detective", "Cute", "Point & Click"], features: SP, critic: Some(83), community: Some(90), ttb: Some((8, 11, 14)), size_gb: 2.8, installed: true, hours: 10.0, last_days: Some(60), status: "beaten", user_score: Some(86), favorite: false, categories: &[], description: "Two cats, one notebook and a neighborhood full of very suspicious dogs.", hue: 20 },
    Demo { name: "Ironclad Seas", stores: &["ubisoft"], developer: "Grand Arc Entertainment", publisher: "Grand Arc Entertainment", year: 2020, genres: &["Strategy", "Simulation"], tags: &["Naval", "Historical"], features: SP, critic: Some(78), community: Some(75), ttb: Some((30, 50, 90)), size_gb: 27.0, installed: false, hours: 0.0, last_days: None, status: "not-played", user_score: None, favorite: false, categories: &["Backlog"], description: "Command steam-age fleets through campaigns of blockades, storms and very large cannons.", hue: 215 },
    Demo { name: "Pocket Galaxy", stores: &["steam"], developer: "Tiny Orbit", publisher: "Tiny Orbit", year: 2023, genres: &["Strategy", "Casual"], tags: &["Idle", "Space", "Incremental"], features: &["Single-player", "Cloud saves"], critic: None, community: Some(86), ttb: None, size_gb: 0.3, installed: true, hours: 140.0, last_days: Some(0), status: "playing", user_score: Some(72), favorite: false, categories: &[], description: "Grow a galaxy that fits in your pocket. Leave it running, come back to new stars.", hue: 280 },
    Demo { name: "Thornwall", stores: &["battlenet"], developer: "Arclight Entertainment", publisher: "Arclight Entertainment", year: 2024, genres: &["Action", "RPG"], tags: &["Hack and Slash", "Loot", "Co-op"], features: COOP, critic: Some(86), community: Some(78), ttb: Some((25, 60, 200)), size_gb: 85.0, installed: true, hours: 96.0, last_days: Some(7), status: "playing", user_score: Some(80), favorite: false, categories: &["Couch co-op"], description: "Fight your way down through a cursed fortress, floor by floor, loot by loot.", hue: 355 },
];

/// Games also owned in a second store, left for Questkeep to suggest combining.
const DUPLICATES: &[(&str, &str)] = &[("Hollow Crown", "gog"), ("Vector Siege", "steam")];

/// Applications in the library (shown with "Kind: Apps"), not games.
const APPS: &[(&str, &str)] = &[("Voice Chat", "chat"), ("Screen Recorder", "recorder")];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir: PathBuf = std::env::args()
        .nth(1)
        .ok_or("usage: demo_library <empty data dir>")?
        .into();
    std::fs::create_dir_all(&dir)?;
    if dir.join("library.sqlite").exists() {
        return Err("the data dir already has a library".into());
    }
    let db = Database::open(&dir.join("library.sqlite"))?;
    // The day the sessions lead up to.
    let now = Utc::now();
    let mut rng = Rng(0x5eed_cafe);

    db.finish_setup()?;
    db.dismiss_igdb_hint()?;
    let statuses = completion_statuses(&db)?;

    let mut by_store: BTreeMap<&str, Vec<(usize, ImportedGame)>> = BTreeMap::new();
    for (index, game) in GAMES.iter().enumerate() {
        for (copy, store) in game.stores.iter().enumerate() {
            let id = format!("demo-{index}-{copy}");
            let folder = if *store == "steam" {
                format!(r"E:\SteamLibrary\steamapps\common\{}", game.name)
            } else {
                format!(r"D:\Games\{}", game.name)
            };
            let installed = game.installed && copy == 0;
            by_store.entry(store).or_default().push((
                index,
                ImportedGame {
                    source_game_id: id.clone(),
                    name: game.name.into(),
                    installed,
                    install_dir: installed.then_some(folder),
                    play_action: Some(PlayAction::uri(format!("questkeep-demo://play/{id}"))),
                    playtime_seconds: None,
                    last_played: None,
                    app: None,
                },
            ));
        }
    }
    // Two games owned twice that are not combined yet (the "Games in several stores" dialog).
    for (index, (name, store)) in DUPLICATES.iter().enumerate() {
        by_store.entry(store).or_default().push((
            usize::MAX,
            ImportedGame {
                source_game_id: format!("demo-dup-{index}"),
                name: (*name).into(),
                installed: false,
                install_dir: None,
                play_action: Some(PlayAction::uri(format!(
                    "questkeep-demo://play/dup-{index}"
                ))),
                playtime_seconds: None,
                last_played: None,
                app: None,
            },
        ));
    }
    let added = now - Duration::days(900);
    for (store, games) in &by_store {
        let imported: Vec<ImportedGame> = games.iter().map(|(_, g)| g.clone()).collect();
        db.upsert_imported(store, &imported, added)?;
    }
    let mut manual = Vec::new();
    for (name, slug) in APPS {
        let app = db.insert_game(
            &questkeep_core::db::NewGameRecord {
                name: (*name).into(),
                source: "manual".into(),
                source_game_id: None,
                install_dir: Some(format!(r"C:\Program Files\{name}")),
                play_action: Some(PlayAction::uri(format!("questkeep-demo://app/{slug}"))),
                cover_path: None,
            },
            added,
        )?;
        db.update_game(
            app.id,
            &questkeep_core::model::GameUpdate {
                kind: Some(questkeep_core::model::GameKind::App),
                ..Default::default()
            },
        )?;
        manual.push(app.id);
    }

    // Rows by (game index, copy).
    let all = db.list_games(&GameQuery {
        hidden: HiddenFilter::All,
        kind: KindFilter::Both,
        ..GameQuery::default()
    })?;
    let row = |index: usize, copy: usize| {
        let wanted = format!("demo-{index}-{copy}");
        all.iter()
            .find(|g| g.source_game_id.as_deref() == Some(wanted.as_str()))
            .map(|g| g.id)
    };

    let sources: Vec<String> = by_store.keys().map(|s| (*s).to_string()).collect();
    let categories = make_categories(&db)?;
    let mut art = Vec::new();
    for (index, game) in GAMES.iter().enumerate() {
        let Some(main) = row(index, 0) else {
            continue;
        };
        for (copy, store) in game.stores.iter().enumerate() {
            if let Some(id) = row(index, copy) {
                db.set_source_metadata(id, store, Some(&metadata(game, store)), added)?;
            }
        }
        let others: Vec<Uuid> = (1..game.stores.len())
            .filter_map(|copy| row(index, copy))
            .collect();
        if !others.is_empty() {
            db.combine_games(main, &others)?;
        }
        sessions(&db, &mut rng, main, game, now)?;
        if let Some(status) = statuses.get(game.status) {
            db.set_completion_status(&[main], Some(status))?;
        }
        db.set_user_score_and_notes(main, game.user_score, None)?;
        if game.favorite {
            db.update_game(
                main,
                &questkeep_core::model::GameUpdate {
                    favorite: Some(true),
                    ..Default::default()
                },
            )?;
        }
        let ids: Vec<String> = game
            .categories
            .iter()
            .filter_map(|c| categories.get(c).cloned())
            .collect();
        if !ids.is_empty() {
            db.set_game_categories(main, &ids)?;
        }
        let cover = format!("images/covers/{main}.png");
        let background = format!("images/backgrounds/{main}.jpg");
        let icon = format!("images/icons/{main}.png");
        db.set_cover_path(main, Some(&cover))?;
        db.set_background_path(main, Some(&background))?;
        db.set_icon_path(main, Some(&icon))?;
        art.push(serde_json::json!({
            "id": main, "name": game.name, "genre": game.genres[0], "hue": game.hue,
            "developer": game.developer, "cover": cover, "background": background, "icon": icon,
        }));
    }
    db.resolve_all(&MetadataPriority::default(), &sources)?;
    // Added over the years, not all on one day (the store import has one date for all).
    let conn = rusqlite::Connection::open(dir.join("library.sqlite"))?;
    for (index, _) in GAMES.iter().enumerate() {
        for copy in 0..3 {
            let days = 20 + (GAMES.len() - index) as i64 * 23 + copy * 41;
            conn.execute(
                "UPDATE games SET date_added = ?2 WHERE source_game_id = ?1",
                rusqlite::params![format!("demo-{index}-{copy}"), now - Duration::days(days)],
            )?;
        }
    }
    drop(conn);
    db.apply_auto_categories(None)?;
    std::fs::write(dir.join("demo-art.json"), serde_json::to_vec_pretty(&art)?)?;
    // Synced a few minutes ago.
    let synced = now - Duration::minutes(12);
    for store in by_store.keys() {
        db.record_sync(Some(store), synced)?;
    }
    db.record_sync(None, synced)?;
    std::fs::write(dir.join("settings.json"), SETTINGS)?;
    println!(
        "{} games, {} apps in {}",
        GAMES.len(),
        manual.len(),
        dir.display()
    );
    Ok(())
}

/// No scans, downloads or launch detection: the demo library stays as it is.
const SETTINGS: &str = r#"{
  "scanOnStartup": false,
  "autoSyncMinutes": 0,
  "watchInstallFolders": false,
  "downloadCovers": false,
  "fetchMetadata": false,
  "detectExternalLaunches": false,
  "language": "en"
}
"#;

fn metadata(game: &Demo, store: &str) -> GameMetadata {
    let slug: String = game
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let store_page = match store {
        "steam" => "Steam store page",
        "gog" => "GOG store page",
        "epic" => "Epic Games store page",
        "xbox" => "Xbox store page",
        "ubisoft" => "Ubisoft Store page",
        "ea" => "EA app page",
        "battlenet" => "Battle.net page",
        _ => "Store page",
    };
    GameMetadata {
        description: Some(game.description.into()),
        release_date: Some(format!(
            "{}-{:02}-{:02}",
            game.year,
            (game.hue % 12) + 1,
            (game.hue % 27) + 1
        )),
        developers: vec![game.developer.into()],
        publishers: vec![game.publisher.into()],
        genres: game.genres.iter().map(|g| (*g).into()).collect(),
        tags: game.tags.iter().map(|g| (*g).into()).collect(),
        features: game.features.iter().map(|g| (*g).into()).collect(),
        platforms: vec!["PC (Windows)".into()],
        critic_score: game.critic.map(|value| Score {
            value,
            source: "Metacritic".into(),
        }),
        community_score: game.community.map(|value| Score {
            value,
            source: if store == "steam" {
                "Steam reviews".into()
            } else {
                "Player reviews".into()
            },
        }),
        install_size: Some((game.size_gb * 1_073_741_824.0) as u64),
        time_to_beat: game.ttb.map(|(main, extra, complete)| TimeToBeat {
            main: Some(main * 3600),
            extra: Some(extra * 3600),
            complete: Some(complete * 3600),
        }),
        links: vec![
            GameLink::new(store_page, format!("https://example.com/{store}/{slug}")),
            GameLink::new(
                "Official website",
                format!("https://example.com/games/{slug}"),
            ),
            GameLink::new("Community wiki", format!("https://example.com/wiki/{slug}")),
        ],
        ..GameMetadata::default()
    }
}

/// The default statuses plus a few more, by name → id.
fn completion_statuses(
    db: &Database,
) -> Result<BTreeMap<&'static str, String>, Box<dyn std::error::Error>> {
    let mut settings: CompletionSettings = db.completion_settings()?;
    for name in ["Playing", "Completed", "On Hold", "Abandoned"] {
        settings.statuses.push(CompletionStatus {
            id: String::new(),
            name: name.into(),
            game_count: 0,
        });
    }
    let saved = db.set_completion_settings(&settings, &BTreeMap::new())?;
    let id = |name: &str| {
        saved
            .statuses
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.id.clone())
            .unwrap_or_default()
    };
    Ok(BTreeMap::from([
        ("not-played", id("Not Played")),
        ("played", id("Played")),
        ("beaten", id("Beaten")),
        ("playing", id("Playing")),
        ("completed", id("Completed")),
        ("abandoned", id("Abandoned")),
    ]))
}

fn make_categories(
    db: &Database,
) -> Result<BTreeMap<&'static str, String>, Box<dyn std::error::Error>> {
    let mut ids = BTreeMap::new();
    for name in ["Backlog", "Couch co-op", "Cozy evenings", "Weekend picks"] {
        ids.insert(name, db.create_category(name)?.id);
    }
    // An automatic one: every racing game.
    let racing = db.create_category("Racing games")?;
    db.set_category_filter(
        &racing.id,
        Some(&serde_json::from_value::<GameQuery>(serde_json::json!({
            "filter": { "mode": "all", "items": [
                { "kind": "list", "field": "genre", "mode": "any", "values": ["racing"] }
            ] }
        }))?),
    )?;
    Ok(ids)
}

/// Sessions adding up to about `game.hours`, ending `game.last_days` ago: evenings on weekdays,
/// afternoons at weekends, denser towards the end.
fn sessions(
    db: &Database,
    rng: &mut Rng,
    game: Uuid,
    demo: &Demo,
    now: DateTime<Utc>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(last_days) = demo.last_days else {
        return Ok(());
    };
    let mut left = demo.hours * 3600.0;
    let mut day = now.date_naive() - Duration::days(last_days);
    // The last session ends before now.
    let mut first = true;
    let mut planned = Vec::new();
    while left > 600.0 {
        let weekend = matches!(day.weekday(), Weekday::Sat | Weekday::Sun);
        let start_hour = if weekend {
            13 + rng.below(8)
        } else {
            18 + rng.below(4)
        };
        let length = (1800 + rng.below(if weekend { 12_600 } else { 7_200 })) as f64;
        let length = length.min(left).max(600.0);
        let start = Utc
            .with_ymd_and_hms(
                day.year(),
                day.month(),
                day.day(),
                start_hour,
                rng.below(60),
                0,
            )
            .single()
            .ok_or("bad date")?;
        let start = if first && start + Duration::seconds(length as i64) > now {
            now - Duration::seconds(length as i64 + 1800)
        } else {
            start
        };
        first = false;
        planned.push((start, length as u64));
        left -= length;
        // Gaps grow the further back it goes: busy weeks recently, the odd evening long ago.
        let gap = 1 + rng.below(if (now.date_naive() - day).num_days() < 60 {
            3
        } else {
            9
        });
        day -= Duration::days(i64::from(gap));
    }
    // Oldest first, so "last played" ends at the newest session.
    for (start, length) in planned.into_iter().rev() {
        let session = db.begin_session(game, start)?;
        db.finish_session(session, start + Duration::seconds(length as i64), length)?;
    }
    Ok(())
}

/// A tiny deterministic generator, so a demo library looks the same every time.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((self.0 >> 33) % u64::from(n.max(1))) as u32
    }
}
