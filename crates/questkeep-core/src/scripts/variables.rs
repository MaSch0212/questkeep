//! What scripts get to know about the game: environment variables (`QK_GAME_NAME`) and the same
//! values as `{GameName}` placeholders in arguments and working folders, plus the complete game
//! record as a JSON file (`QK_GAME_JSON`).

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::launcher::LaunchOutcome;
use crate::model::{Game, ScriptKind, ScriptVariable, VariableGroup};

/// Everything a script run knows.
pub struct Context<'a> {
    /// The library entry (images resolved to absolute paths).
    pub entry: &'a Game,
    /// The copy that was launched (with its launch profile applied), for pre and post scripts.
    pub launched: Option<&'a Game>,
    pub phase: ScriptKind,
    pub script_name: &'a str,
    pub script_file: Option<&'a Path>,
    pub data_dir: &'a Path,
    pub game_json: Option<&'a Path>,
    /// Names of the entry's categories.
    pub categories: &'a [String],
    pub store_name: &'a str,
    /// Post scripts: how the session went.
    pub outcome: Option<&'a LaunchOutcome>,
}

/// `QK_GAME_NAME` → `GameName`.
fn placeholder(env: &str) -> String {
    env.trim_start_matches("QK_")
        .split('_')
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            let mut chars = lower.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn time(t: Option<DateTime<Utc>>) -> String {
    t.map(|t| t.to_rfc3339()).unwrap_or_default()
}

fn duration(seconds: u64) -> String {
    let (h, m) = (seconds / 3600, (seconds % 3600) / 60);
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// The variables of a run, grouped and in panel order.
pub fn variables(ctx: &Context<'_>) -> Vec<ScriptVariable> {
    let g = ctx.entry;
    let launched = ctx.launched.unwrap_or(g);
    let action = launched.play_action.as_ref();
    let list = |items: &[String]| items.join(";");
    let opt = |v: &Option<String>| v.clone().unwrap_or_default();
    let mut out = Vec::new();
    let mut add = |group: VariableGroup, env: &str, value: String| {
        out.push(ScriptVariable {
            env: env.to_string(),
            placeholder: placeholder(env),
            group,
            value,
        });
    };
    use VariableGroup::{
        Context as C, Game as G, Images as I, Launch as L, Lists as Li, Session as S,
    };

    add(C, "QK_PHASE", ctx.phase.as_str().to_string());
    add(C, "QK_SCRIPT_NAME", ctx.script_name.to_string());
    add(
        C,
        "QK_SCRIPT_FILE",
        ctx.script_file
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    );
    add(C, "QK_DATA_DIR", ctx.data_dir.display().to_string());
    add(
        C,
        "QK_GAME_JSON",
        ctx.game_json
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    );

    add(G, "QK_GAME_ID", g.id.to_string());
    add(G, "QK_GAME_NAME", g.name.clone());
    add(G, "QK_SORTING_NAME", g.sorting_name.clone());
    add(G, "QK_GAME_KIND", g.kind.as_str().to_string());
    add(G, "QK_STORE", g.source.clone());
    add(G, "QK_STORE_NAME", ctx.store_name.to_string());
    add(G, "QK_STORE_GAME_ID", opt(&g.source_game_id));
    add(G, "QK_INSTALLED", g.installed.to_string());
    add(G, "QK_INSTALL_DIR", opt(&launched.install_dir));
    add(G, "QK_FAVORITE", g.favorite.to_string());
    add(G, "QK_HIDDEN", g.hidden.to_string());
    add(G, "QK_COMPLETION_STATUS", opt(&g.completion_status));
    add(
        G,
        "QK_USER_SCORE",
        g.user_score.map(|s| s.to_string()).unwrap_or_default(),
    );
    add(
        G,
        "QK_CRITIC_SCORE",
        g.critic_score
            .as_ref()
            .map(|s| s.value.to_string())
            .unwrap_or_default(),
    );
    add(
        G,
        "QK_COMMUNITY_SCORE",
        g.community_score
            .as_ref()
            .map(|s| s.value.to_string())
            .unwrap_or_default(),
    );
    add(G, "QK_RELEASE_DATE", opt(&g.release_date));
    add(G, "QK_PLAYTIME_SECONDS", g.playtime_seconds.to_string());
    add(G, "QK_PLAYTIME", duration(g.playtime_seconds));
    add(G, "QK_PLAY_COUNT", g.play_count.to_string());
    add(G, "QK_LAST_PLAYED", time(g.last_played));
    add(G, "QK_DATE_ADDED", g.date_added.to_rfc3339());
    add(
        G,
        "QK_INSTALL_SIZE",
        g.install_size.map(|s| s.to_string()).unwrap_or_default(),
    );

    add(Li, "QK_GENRES", list(&g.genres));
    add(Li, "QK_DEVELOPERS", list(&g.developers));
    add(Li, "QK_PUBLISHERS", list(&g.publishers));
    add(Li, "QK_TAGS", list(&g.tags));
    add(Li, "QK_FEATURES", list(&g.features));
    add(Li, "QK_SERIES", list(&g.series));
    add(Li, "QK_PLATFORMS", list(&g.platforms));
    add(Li, "QK_CATEGORIES", list(ctx.categories));
    add(Li, "QK_GAME_MODES", list(&g.game_modes));
    add(Li, "QK_THEMES", list(&g.themes));
    add(Li, "QK_PERSPECTIVES", list(&g.perspectives));
    add(Li, "QK_AGE_RATINGS", list(&g.age_ratings));

    add(I, "QK_COVER_PATH", opt(&g.cover_path));
    add(I, "QK_BACKGROUND_PATH", opt(&g.background_path));
    add(I, "QK_ICON_PATH", opt(&g.icon_path));

    add(
        L,
        "QK_LAUNCH_KIND",
        action
            .map(|a| format!("{:?}", a.kind).to_ascii_lowercase())
            .unwrap_or_default(),
    );
    add(
        L,
        "QK_LAUNCH_TARGET",
        action.map(|a| a.target.clone()).unwrap_or_default(),
    );
    add(
        L,
        "QK_LAUNCH_ARGS",
        action.and_then(|a| a.args.clone()).unwrap_or_default(),
    );
    add(
        L,
        "QK_LAUNCH_WORKING_DIR",
        action
            .and_then(|a| a.working_dir.clone())
            .unwrap_or_default(),
    );
    add(L, "QK_LAUNCHED_COPY_ID", launched.id.to_string());
    add(L, "QK_LAUNCHED_STORE", launched.source.clone());
    add(
        L,
        "QK_LAUNCHED_STORE_GAME_ID",
        opt(&launched.source_game_id),
    );

    let outcome = ctx.outcome;
    add(
        S,
        "QK_SESSION_SECONDS",
        outcome.map(|o| o.seconds.to_string()).unwrap_or_default(),
    );
    add(
        S,
        "QK_SESSION_START",
        time(outcome.and_then(|o| o.started_at)),
    );
    add(
        S,
        "QK_SESSION_END",
        outcome.map(|o| o.ended_at.to_rfc3339()).unwrap_or_default(),
    );
    add(
        S,
        "QK_GAME_RAN",
        outcome.map(|o| o.ran.to_string()).unwrap_or_default(),
    );
    out
}

/// Replaces `{Name}` placeholders (case-insensitive) with their values; unknown ones stay as
/// typed.
pub fn expand(text: &str, vars: &[ScriptVariable]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match vars
                    .iter()
                    .find(|v| v.placeholder.eq_ignore_ascii_case(name))
                {
                    Some(var) => out.push_str(&var.value),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The complete record for `QK_GAME_JSON`.
pub fn game_json(ctx: &Context<'_>, vars: &[ScriptVariable]) -> serde_json::Value {
    json!({
        "phase": ctx.phase,
        "game": ctx.entry,
        "launched": ctx.launched,
        "categories": ctx.categories,
        "session": ctx.outcome.map(|o| json!({
            "ran": o.ran,
            "tracked": o.tracked,
            "seconds": o.seconds,
            "startedAt": o.started_at,
            "endedAt": o.ended_at,
        })),
        "variables": vars
            .iter()
            .map(|v| (v.env.clone(), serde_json::Value::String(v.value.clone())))
            .collect::<serde_json::Map<_, _>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(placeholder: &str, value: &str) -> ScriptVariable {
        ScriptVariable {
            env: String::new(),
            placeholder: placeholder.into(),
            group: VariableGroup::Game,
            value: value.into(),
        }
    }

    #[test]
    fn names_placeholders_after_variables() {
        assert_eq!(placeholder("QK_GAME_NAME"), "GameName");
        assert_eq!(
            placeholder("QK_LAUNCHED_STORE_GAME_ID"),
            "LaunchedStoreGameId"
        );
    }

    #[test]
    fn expands_known_placeholders_only() {
        let vars = [
            var("GameName", "Portal 2"),
            var("InstallDir", "D:\\Games\\P2"),
        ];
        assert_eq!(
            expand(r#""{installdir}\saves" {GameName} {Nope} {"#, &vars),
            r#""D:\Games\P2\saves" Portal 2 {Nope} {"#
        );
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration(59), "0m");
        assert_eq!(duration(3 * 3600 + 120), "3h 2m");
    }
}
