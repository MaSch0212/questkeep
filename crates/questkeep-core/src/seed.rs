//! Dev-only fake library generator for performance checks.

use crate::db::NewGameRecord;
use crate::model::PlayAction;

/// Source id used for fake games so they can be told apart (and filtered) easily.
pub const FAKE_SOURCE: &str = "fake";

/// Whether a game comes from the fake library. Such games never get covers, icons or metadata:
/// their invented names would only cost SteamGridDB and IGDB requests that find nothing.
pub fn is_fake_source(source: &str) -> bool {
    source == FAKE_SOURCE
}

const ADJECTIVES: [&str; 16] = [
    "Ancient",
    "Broken",
    "Crimson",
    "Distant",
    "Eternal",
    "Forgotten",
    "Golden",
    "Hidden",
    "Iron",
    "Jade",
    "Lost",
    "Midnight",
    "Neon",
    "Silent",
    "Velvet",
    "Wild",
];
const NOUNS: [&str; 16] = [
    "Kingdom",
    "Odyssey",
    "Frontier",
    "Legacy",
    "Horizon",
    "Citadel",
    "Echo",
    "Voyage",
    "Empire",
    "Dungeon",
    "Galaxy",
    "Harbor",
    "Labyrinth",
    "Outpost",
    "Requiem",
    "Tide",
];

/// Deterministic fake games with varied, searchable names.
pub fn fake_games(count: usize) -> Vec<NewGameRecord> {
    (0..count)
        .map(|i| {
            let adjective = ADJECTIVES[i % ADJECTIVES.len()];
            let noun = NOUNS[(i / ADJECTIVES.len()) % NOUNS.len()];
            let sequel = i / (ADJECTIVES.len() * NOUNS.len());
            let name = if sequel == 0 {
                format!("{adjective} {noun}")
            } else {
                format!("{adjective} {noun} {}", sequel + 1)
            };
            NewGameRecord {
                name,
                source: FAKE_SOURCE.into(),
                source_game_id: None,
                install_dir: None,
                play_action: Some(PlayAction::uri(format!("fake://{i}"))),
                cover_path: None,
            }
        })
        .collect()
}
