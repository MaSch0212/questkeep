//! The wire format between the app and a store plugin. Both sides use this one module, so they
//! cannot drift apart.
//!
//! - `describe()` answers a [`ProviderDescriptor`] as JSON.
//! - `call(method, args)`: `method` is a [`Method`] name (the provider method in snake_case),
//!   `args` one JSON object ([`CallArgs`], camelCase; fields a method does not use are left out).
//!   The answer is the JSON of the method's return value: for example a list of `ImportedGame`
//!   for `scan_installed`, a `PlayAction` or `null` for `install_action`, `true`/`false` for
//!   `covers_are_low_quality`, `null` for `configure`. Errors travel as the WIT `provider-error`.
//!
//! | Method | Arguments | Answer |
//! |---|---|---|
//! | `scan_installed` | | `ImportedGame[]` |
//! | `scan_owned` | `config` | `ImportedGame[]` |
//! | `scan_categories` | `config` | `{ gameId: string[] } \| null` |
//! | `install_action`, `uninstall_action`, `launcher_page` | `gameId` | `PlayAction \| null` |
//! | `launch_with_args` | `gameId`, `args` | `PlayAction \| null` |
//! | `metadata` | `gameId` | `GameMetadata \| null` |
//! | `player_count` | `gameId` | `number \| null` |
//! | `news` | `gameId` | `GameNews \| null` |
//! | `achievements` | `config`, `gameId` | `Achievements \| null` |
//! | `cover_candidates`, `background_candidates`, `icon_candidates` | `gameId` | `ImageCandidate[]` |
//! | `covers_are_low_quality` | `gameId` | `boolean` |
//! | `has_owned_source` | `config` | `boolean` |
//! | `configure` | `config` | `null` |
//! | `run_add_game_option` | `optionId` | `null` |
//! | `account_status` | | `AccountInfo` |
//! | `sign_in_with_code` | `input` | `null` |
//! | `sign_out` | | `null` |

use serde::{Deserialize, Serialize};

use crate::model::{AddGameOption, LauncherProcess, PlayAction};
use crate::settings::StoreSettings;

/// The provider methods that reach the plugin through `call`. Everything else is answered by the
/// app from the [`ProviderDescriptor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    ScanInstalled,
    ScanOwned,
    ScanCategories,
    InstallAction,
    UninstallAction,
    LauncherPage,
    LaunchWithArgs,
    Metadata,
    PlayerCount,
    News,
    Achievements,
    CoverCandidates,
    CoversAreLowQuality,
    BackgroundCandidates,
    IconCandidates,
    HasOwnedSource,
    Configure,
    RunAddGameOption,
    AccountStatus,
    SignInWithCode,
    SignOut,
}

impl Method {
    pub const ALL: [Method; 21] = [
        Method::ScanInstalled,
        Method::ScanOwned,
        Method::ScanCategories,
        Method::InstallAction,
        Method::UninstallAction,
        Method::LauncherPage,
        Method::LaunchWithArgs,
        Method::Metadata,
        Method::PlayerCount,
        Method::News,
        Method::Achievements,
        Method::CoverCandidates,
        Method::CoversAreLowQuality,
        Method::BackgroundCandidates,
        Method::IconCandidates,
        Method::HasOwnedSource,
        Method::Configure,
        Method::RunAddGameOption,
        Method::AccountStatus,
        Method::SignInWithCode,
        Method::SignOut,
    ];

    /// The name on the wire: the provider method in snake_case.
    pub fn name(self) -> &'static str {
        match self {
            Method::ScanInstalled => "scan_installed",
            Method::ScanOwned => "scan_owned",
            Method::ScanCategories => "scan_categories",
            Method::InstallAction => "install_action",
            Method::UninstallAction => "uninstall_action",
            Method::LauncherPage => "launcher_page",
            Method::LaunchWithArgs => "launch_with_args",
            Method::Metadata => "metadata",
            Method::PlayerCount => "player_count",
            Method::News => "news",
            Method::Achievements => "achievements",
            Method::CoverCandidates => "cover_candidates",
            Method::CoversAreLowQuality => "covers_are_low_quality",
            Method::BackgroundCandidates => "background_candidates",
            Method::IconCandidates => "icon_candidates",
            Method::HasOwnedSource => "has_owned_source",
            Method::Configure => "configure",
            Method::RunAddGameOption => "run_add_game_option",
            Method::AccountStatus => "account_status",
            Method::SignInWithCode => "sign_in_with_code",
            Method::SignOut => "sign_out",
        }
    }

    pub fn from_name(name: &str) -> Option<Method> {
        Method::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// The arguments of a `call`. Every method reads the fields it needs (see the module docs).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CallArgs {
    /// The provider's section of the user settings (`null` when unset).
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub config: serde_json::Value,
    /// The store's id of the game.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub game_id: String,
    /// Command line arguments (`launch_with_args`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub args: String,
    /// The add-game option to run (`run_add_game_option`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub option_id: String,
    /// What the sign-in window captured, or the user pasted (`sign_in_with_code`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub input: String,
}

impl CallArgs {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn config(config: &serde_json::Value) -> Self {
        Self {
            config: config.clone(),
            ..Self::default()
        }
    }

    pub fn game(game_id: &str) -> Self {
        Self {
            game_id: game_id.to_string(),
            ..Self::default()
        }
    }
}

/// What the app knows about a provider without calling it: asked when the plugin is loaded and
/// again after every `configure`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub icon_svg: String,
    pub can_scan: bool,
    pub can_scan_owned: bool,
    pub can_uninstall: bool,
    pub has_launcher_pages: bool,
    pub shop_url: Option<String>,
    pub launcher: Option<PlayAction>,
    pub launcher_process: Option<LauncherProcess>,
    pub installed: bool,
    pub settings: StoreSettings,
    pub add_game_options: Vec<AddGameOption>,
    /// The store has an account the user can sign in to (needs `permissions.login`).
    pub has_account: bool,
}

/// A possible source for a game image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageCandidate {
    /// A file the plugin can see: a path below `/plugin`, `/data` or one of its read mounts.
    File(String),
    Url(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_names_round_trip() {
        for method in Method::ALL {
            assert_eq!(Method::from_name(method.name()), Some(method));
        }
        assert_eq!(Method::from_name("nope"), None);
    }

    #[test]
    fn args_leave_out_unused_fields() {
        assert_eq!(serde_json::to_string(&CallArgs::none()).unwrap(), "{}");
        assert_eq!(
            serde_json::to_string(&CallArgs::game("7")).unwrap(),
            r#"{"gameId":"7"}"#
        );
        let parsed: CallArgs = serde_json::from_str(r#"{"config":{"a":true}}"#).unwrap();
        assert_eq!(parsed.config["a"], true);
        assert!(parsed.game_id.is_empty());
    }

    #[test]
    fn image_candidates_are_tagged() {
        assert_eq!(
            serde_json::to_string(&ImageCandidate::Url("https://x/y.png".into())).unwrap(),
            r#"{"url":"https://x/y.png"}"#
        );
    }
}
