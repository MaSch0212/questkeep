//! Game scripts: programs or scripts run before a game starts (pre), after it closes (post), or
//! on demand from a game's page (actions). See [`crate::scripts`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::GameQuery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ScriptKind {
    /// Before a game started from Questkeep starts.
    Pre,
    /// After a game started from Questkeep closed.
    Post,
    /// A button on the game's page and an entry in its context menu.
    Action,
}

impl ScriptKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pre => "pre",
            Self::Post => "post",
            Self::Action => "action",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pre" => Some(Self::Pre),
            "post" => Some(Self::Post),
            "action" => Some(Self::Action),
            _ => None,
        }
    }
}

/// When a pre script runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PreTiming {
    /// Before the game's process is started.
    #[default]
    BeforeStart,
    /// Once the game is seen running (games with their own launcher).
    OnceRunning,
}

/// A scripting runtime Questkeep can run script files with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RuntimeId {
    /// PowerShell 7+ (`pwsh`).
    Pwsh,
    /// Windows PowerShell 5.1 (`powershell.exe`, part of Windows).
    WindowsPowershell,
    Bash,
    /// The Windows Command Prompt (`cmd.exe`).
    Cmd,
    Python,
}

impl RuntimeId {
    pub const ALL: [Self; 5] = [
        Self::Pwsh,
        Self::WindowsPowershell,
        Self::Bash,
        Self::Cmd,
        Self::Python,
    ];

    /// The file extension of its scripts (without the dot).
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pwsh | Self::WindowsPowershell => "ps1",
            Self::Bash => "sh",
            Self::Cmd => "cmd",
            Self::Python => "py",
        }
    }
}

/// What a script runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ScriptSource {
    /// A program: `program` is its executable.
    #[default]
    Program,
    /// A script written in Questkeep (or imported), kept in the scripts folder and run with
    /// `runtime`.
    Inline,
    /// A script file used where it is (`program`), run with `runtime`.
    Linked,
}

/// Advanced options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ScriptOptions {
    /// Show the program's window (or console); hidden by default.
    pub show_window: bool,
    /// Wait until it ends before going on (pre and post scripts).
    pub wait: bool,
    /// Stop waiting after this many seconds (0: wait as long as it runs). The script keeps
    /// running.
    pub timeout_seconds: u32,
    /// A pre script that fails stops the launch.
    pub stop_launch_on_failure: bool,
}

impl Default for ScriptOptions {
    fn default() -> Self {
        Self {
            show_window: false,
            wait: true,
            timeout_seconds: 60,
            stop_launch_on_failure: false,
        }
    }
}

/// A script as saved and shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Script {
    #[ts(type = "string")]
    pub id: Uuid,
    pub name: String,
    pub kind: ScriptKind,
    pub enabled: bool,
    /// The library entry whose own script this is; `None` for global and filtered scripts.
    #[ts(type = "string | null")]
    pub game_id: Option<Uuid>,
    /// Only games matching it (global scripts without a game); `None`: all games.
    pub filter: Option<GameQuery>,
    #[serde(flatten)]
    pub spec: ScriptSpec,
    /// Place in its list (global and filtered scripts share one order per kind; a game's own
    /// scripts have their own).
    pub position: i64,
}

/// Everything about a script that isn't its identity or place: what and how it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ScriptSpec {
    pub timing: PreTiming,
    pub source: ScriptSource,
    /// The executable (`program`) or the linked script file (`linked`); unused for inline.
    pub program: String,
    /// The runtime of inline and linked scripts; `None` picks one from the file extension.
    pub runtime: Option<RuntimeId>,
    /// Arguments as typed; `{Name}` placeholders are replaced by the game's values.
    pub args: String,
    /// The working folder (placeholders allowed); `None`: the game's install folder.
    pub working_dir: Option<String>,
    pub options: ScriptOptions,
    /// The icon of an action's button (a Lucide icon name).
    pub icon: Option<String>,
    /// The text of an inline script (read from and written to its file).
    pub content: Option<String>,
}

/// A script to create or replace. `id` `None` creates one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScriptInput {
    #[ts(type = "string | null")]
    pub id: Option<Uuid>,
    pub name: String,
    pub kind: ScriptKind,
    pub enabled: bool,
    #[ts(type = "string | null")]
    pub game_id: Option<Uuid>,
    pub filter: Option<GameQuery>,
    #[serde(flatten)]
    pub spec: ScriptSpec,
}

/// One row of the pre or post order: a global/filtered script, or where the game's own scripts
/// run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ScriptSlot {
    Script {
        #[ts(type = "string")]
        id: Uuid,
    },
    GameScripts,
}

/// The global scripts and their order, for Settings › Scripts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GlobalScripts {
    pub scripts: Vec<Script>,
    /// The pre scripts in run order, with the place of the games' own scripts.
    pub pre_order: Vec<ScriptSlot>,
    pub post_order: Vec<ScriptSlot>,
    /// How many games have own pre scripts.
    pub games_with_pre_scripts: u32,
    /// How many games have own post scripts.
    pub games_with_post_scripts: u32,
}

/// The scripts of one game, for the Edit dialog and the game page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameScripts {
    /// The game's own scripts.
    pub own: Vec<Script>,
    /// Whether global and filtered scripts run for it.
    pub run_global: bool,
    /// Every script that applies to it, in run order per kind (pre, post, actions); own scripts
    /// included. Disabled scripts are left out.
    pub applied: Vec<Script>,
}

/// A runtime found on this PC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimeInfo {
    pub id: RuntimeId,
    /// e.g. "PowerShell 7", "Windows PowerShell 5.1".
    pub name: String,
    pub path: String,
    pub extension: String,
}

/// A variable a script gets, with its value for a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScriptVariable {
    /// Environment variable name (`QK_GAME_NAME`).
    pub env: String,
    /// Placeholder name for arguments (`GameName`, used as `{GameName}`).
    pub placeholder: String,
    /// Group in the variables panel ("Game", "Images", …).
    pub group: VariableGroup,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum VariableGroup {
    Context,
    Game,
    Lists,
    Images,
    Launch,
    Session,
}
