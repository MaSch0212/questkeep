//! Pure logic behind the system tray and the window lifecycle: which games the tray menu lists,
//! when the close/minimize buttons hide the app instead, when the process may exit, and how the
//! `--minimized` launch argument is read. The Tauri shell only applies these decisions.

use std::path::PathBuf;

use uuid::Uuid;

use crate::model::Game;
use crate::settings::Settings;

/// Games in the "recently played" part of the tray menu.
pub const RECENT_LIMIT: usize = 10;
/// Games in the tray's favorites submenu.
pub const FAVORITES_LIMIT: usize = 30;
/// Longest game name shown in the menu (longer ones are cut with an ellipsis).
const MAX_LABEL_CHARS: usize = 60;

/// Launch argument the autostart entry passes: start in the tray without a window.
pub const MINIMIZED_ARG: &str = "--minimized";

/// One game in the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayGame {
    pub id: Uuid,
    pub name: String,
    /// The game's cached 32x32 icon. [`tray_games`] copies the stored (data-dir relative) path;
    /// [`crate::library::Library::tray_games`] makes it absolute and drops icons whose file is
    /// gone. Part of the comparison, so a newly made icon rebuilds the menu.
    pub icon: Option<PathBuf>,
    /// The game's cached cover, resolved like [`TrayGame::icon`]; the tray popup shows recent
    /// games as cover tiles.
    pub cover: Option<PathBuf>,
}

/// The game lists of the tray menu. Compared to skip menu rebuilds when nothing changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayGames {
    /// Most recently played first.
    pub recent: Vec<TrayGame>,
    /// By name.
    pub favorites: Vec<TrayGame>,
}

fn listable(game: &Game) -> bool {
    game.installed && !game.hidden
}

fn entry(game: &Game) -> TrayGame {
    TrayGame {
        id: game.id,
        name: game.name.clone(),
        icon: game.icon_path.as_ref().map(PathBuf::from),
        cover: game.cover_path.as_ref().map(PathBuf::from),
    }
}

/// Picks the games for the tray menu from `games` (any order): recently played installed,
/// non-hidden games (newest first, at most [`RECENT_LIMIT`]) and favorite installed, non-hidden
/// games sorted by name (at most [`FAVORITES_LIMIT`]).
pub fn tray_games(games: &[Game]) -> TrayGames {
    let mut recent: Vec<&Game> = games
        .iter()
        .filter(|g| listable(g) && g.last_played.is_some())
        .collect();
    recent.sort_by(|a, b| {
        b.last_played
            .cmp(&a.last_played)
            .then_with(|| {
                a.sorting_name
                    .to_lowercase()
                    .cmp(&b.sorting_name.to_lowercase())
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut favorites: Vec<&Game> = games.iter().filter(|g| listable(g) && g.favorite).collect();
    favorites.sort_by(|a, b| {
        a.sorting_name
            .to_lowercase()
            .cmp(&b.sorting_name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    TrayGames {
        recent: recent.into_iter().take(RECENT_LIMIT).map(entry).collect(),
        favorites: favorites
            .into_iter()
            .take(FAVORITES_LIMIT)
            .map(entry)
            .collect(),
    }
}

impl TrayGames {
    /// Ids of the listed games that have no icon yet (each once), for the icon fetcher.
    pub fn missing_icons(&self) -> Vec<Uuid> {
        let mut ids: Vec<Uuid> = Vec::new();
        for game in self.recent.iter().chain(&self.favorites) {
            if game.icon.is_none() && !ids.contains(&game.id) {
                ids.push(game.id);
            }
        }
        ids
    }

    /// Replaces every icon and cover path with `resolve(path)` (`None` drops it).
    pub fn map_images(mut self, resolve: impl Fn(&std::path::Path) -> Option<PathBuf>) -> Self {
        for game in self.recent.iter_mut().chain(&mut self.favorites) {
            game.icon = game.icon.take().and_then(|p| resolve(&p));
            game.cover = game.cover.take().and_then(|p| resolve(&p));
        }
        self
    }
}

/// A game name as menu text: cut to a sensible length and with `&` doubled, because menus treat a
/// single `&` as a keyboard-mnemonic marker.
pub fn menu_label(name: &str) -> String {
    let name = name.trim();
    let shown: String = if name.chars().count() > MAX_LABEL_CHARS {
        let cut: String = name.chars().take(MAX_LABEL_CHARS - 1).collect();
        format!("{}\u{2026}", cut.trim_end())
    } else {
        name.to_string()
    };
    shown.replace('&', "&&")
}

/// What the window's close button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Close the window and keep running in the tray.
    HideToTray,
    /// Close the window; the app exits with it.
    Quit,
}

/// `tray_available` is false when the tray icon could not be created: hiding then would leave no
/// way back to the app, so the close button quits.
pub fn close_action(settings: &Settings, tray_available: bool) -> CloseAction {
    if settings.close_to_tray && tray_available {
        CloseAction::HideToTray
    } else {
        CloseAction::Quit
    }
}

/// Whether minimizing the window hides the app to the tray.
pub fn hide_on_minimize(settings: &Settings, tray_available: bool) -> bool {
    settings.minimize_to_tray && tray_available
}

/// Whether the process must stay alive after the last window went away. Only the implicit exit
/// (`code == None`, no window left) is ever prevented, and only while the app waits in the tray
/// and no real exit (tray "Exit") was requested.
pub fn keep_running_without_windows(
    code: Option<i32>,
    hidden_to_tray: bool,
    quit_requested: bool,
) -> bool {
    code.is_none() && hidden_to_tray && !quit_requested
}

/// Facts read from the process command line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaunchArgs {
    /// Started by the system at sign-in (`--minimized`).
    pub minimized: bool,
}

/// Reads the launch arguments (without the program name or with it; it is ignored either way).
pub fn parse_launch_args<I, S>(args: I) -> LaunchArgs
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    LaunchArgs {
        minimized: args
            .into_iter()
            .any(|a| a.as_ref().eq_ignore_ascii_case(MINIMIZED_ARG)),
    }
}

/// Whether startup should skip the main window and stay in the tray.
pub fn start_in_tray(settings: &Settings, args: LaunchArgs, tray_available: bool) -> bool {
    args.minimized && settings.start_minimized && tray_available
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use super::*;
    use crate::model::Game;

    fn game(name: &str) -> Game {
        let mut g = Game {
            id: Uuid::new_v4(),
            name: name.to_string(),
            sorting_name: name.to_lowercase(),
            source: "manual".into(),
            source_game_id: None,
            installed: true,
            install_dir: None,
            play_action: None,
            playtime_seconds: 0,
            play_count: 0,
            last_played: None,
            date_added: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            favorite: false,
            hidden: false,
            cover_path: None,
            icon_path: None,
            background_path: None,
            description: None,
            release_date: None,
            developers: vec![],
            publishers: vec![],
            genres: vec![],
            metadata_updated_at: None,
            extra: serde_json::Value::Null,
        };
        g.name = name.to_string();
        g
    }

    fn played(name: &str, days_ago: i64) -> Game {
        let mut g = game(name);
        g.last_played =
            Some(Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap() - Duration::days(days_ago));
        g
    }

    fn names(list: &[TrayGame]) -> Vec<&str> {
        list.iter().map(|g| g.name.as_str()).collect()
    }

    #[test]
    fn recent_lists_newest_installed_visible_games_only() {
        let mut uninstalled = played("Gone", 0);
        uninstalled.installed = false;
        let mut hidden = played("Secret", 0);
        hidden.hidden = true;
        let games = vec![
            played("Old", 9),
            played("New", 1),
            game("Never played"),
            uninstalled,
            hidden,
            played("Mid", 5),
        ];
        let picked = tray_games(&games);
        assert_eq!(names(&picked.recent), ["New", "Mid", "Old"]);
    }

    #[test]
    fn recent_is_capped_and_ties_break_by_name() {
        let games: Vec<Game> = (0..15).map(|i| played(&format!("G{i:02}"), 3)).collect();
        let picked = tray_games(&games);
        assert_eq!(picked.recent.len(), RECENT_LIMIT);
        assert_eq!(picked.recent[0].name, "G00");
        assert_eq!(picked.recent[9].name, "G09");
    }

    #[test]
    fn favorites_are_sorted_by_name_filtered_and_capped() {
        let mut games: Vec<Game> = Vec::new();
        for name in ["zelda", "Alpha", "beta"] {
            let mut g = game(name);
            g.favorite = true;
            games.push(g);
        }
        let mut hidden = game("Hidden fav");
        hidden.favorite = true;
        hidden.hidden = true;
        let mut uninstalled = game("Gone fav");
        uninstalled.favorite = true;
        uninstalled.installed = false;
        games.extend([hidden, uninstalled, game("Not a favorite")]);
        assert_eq!(
            names(&tray_games(&games).favorites),
            ["Alpha", "beta", "zelda"]
        );

        let many: Vec<Game> = (0..40)
            .map(|i| {
                let mut g = game(&format!("F{i:02}"));
                g.favorite = true;
                g
            })
            .collect();
        let picked = tray_games(&many);
        assert_eq!(picked.favorites.len(), FAVORITES_LIMIT);
        assert_eq!(picked.favorites[0].name, "F00");
    }

    #[test]
    fn entries_carry_icon_paths_and_report_missing_ones() {
        let mut with_icon = played("With icon", 1);
        with_icon.icon_path = Some("images/icons/a.png".into());
        let mut fav = game("Fav without icon");
        fav.favorite = true;
        // Recent and favorite at once: listed twice, reported once.
        let mut both = played("Both", 2);
        both.favorite = true;
        let games = vec![with_icon, fav.clone(), both.clone()];
        let picked = tray_games(&games);
        assert_eq!(
            picked.recent[0].icon.as_deref(),
            Some(std::path::Path::new("images/icons/a.png"))
        );
        assert_eq!(picked.recent[1].icon, None);
        assert_eq!(picked.missing_icons(), vec![both.id, fav.id]);

        // A new icon changes the model, so the shell rebuilds the menu.
        let mut games2 = games.clone();
        games2[2].icon_path = Some("images/icons/b.png".into());
        assert_ne!(tray_games(&games2), picked);
        assert_eq!(tray_games(&games), picked);
        assert_eq!(tray_games(&games2).missing_icons(), vec![fav.id]);

        let resolved = picked
            .map_images(|p| (p.file_name()? == "a.png").then(|| PathBuf::from("C:/data").join(p)));
        assert_eq!(
            resolved.recent[0].icon,
            Some(PathBuf::from("C:/data").join("images/icons/a.png"))
        );
    }

    #[test]
    fn empty_library_gives_empty_menu_lists() {
        assert_eq!(tray_games(&[]), TrayGames::default());
    }

    #[test]
    fn labels_escape_ampersands_and_cut_long_names() {
        assert_eq!(menu_label("  Tom & Jerry "), "Tom && Jerry");
        let long = "x".repeat(100);
        let label = menu_label(&long);
        assert_eq!(label.chars().count(), MAX_LABEL_CHARS);
        assert!(label.ends_with('\u{2026}'));
        assert_eq!(menu_label("short"), "short");
    }

    #[test]
    fn close_and_minimize_follow_the_settings() {
        let on = Settings::default();
        assert!(on.close_to_tray);
        assert_eq!(close_action(&on, true), CloseAction::HideToTray);
        assert_eq!(close_action(&on, false), CloseAction::Quit);
        let off = Settings {
            close_to_tray: false,
            ..Settings::default()
        };
        assert_eq!(close_action(&off, true), CloseAction::Quit);

        assert!(!hide_on_minimize(&Settings::default(), true));
        let min = Settings {
            minimize_to_tray: true,
            ..Settings::default()
        };
        assert!(hide_on_minimize(&min, true));
        assert!(!hide_on_minimize(&min, false));
    }

    #[test]
    fn only_the_implicit_exit_is_prevented_while_waiting_in_the_tray() {
        assert!(keep_running_without_windows(None, true, false));
        // The user closed the window with "close to tray" off: let the app exit.
        assert!(!keep_running_without_windows(None, false, false));
        // A real exit (tray "Exit", update restart) goes through.
        assert!(!keep_running_without_windows(Some(0), true, false));
        assert!(!keep_running_without_windows(None, true, true));
    }

    #[test]
    fn minimized_argument_is_detected() {
        assert!(!parse_launch_args(["questkeep.exe"]).minimized);
        assert!(parse_launch_args(["questkeep.exe", "--minimized"]).minimized);
        assert!(parse_launch_args(vec!["--MINIMIZED".to_string()]).minimized);
        assert!(!parse_launch_args(["--minimized-ish", "minimized"]).minimized);
        assert!(!parse_launch_args(Vec::<String>::new()).minimized);
    }

    #[test]
    fn starting_in_the_tray_needs_the_argument_the_setting_and_a_tray() {
        let args = LaunchArgs { minimized: true };
        let s = Settings::default();
        assert!(start_in_tray(&s, args, true));
        assert!(!start_in_tray(&s, LaunchArgs::default(), true));
        assert!(!start_in_tray(&s, args, false));
        let off = Settings {
            start_minimized: false,
            ..Settings::default()
        };
        assert!(!start_in_tray(&off, args, true));
    }
}
