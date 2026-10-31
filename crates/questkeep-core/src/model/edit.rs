//! What the Edit game dialog reads and writes: the user's own metadata values, links, images and
//! launch profiles.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::{GameLink, GameMetadata, MetadataField, PlayAction, url_key};

/// The user's own metadata of a library entry, stored as the "user" source. It wins over every
/// other source for the fields in `edited`, even when the value is empty ("no tags").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct UserEdits {
    /// The values of the edited fields; `meta.links` are the user's own links.
    pub meta: GameMetadata,
    pub edited: Vec<MetadataField>,
    /// URLs of links from other sources the user hid.
    pub hidden_links: Vec<String>,
}

impl UserEdits {
    pub fn is_edited(&self, field: MetadataField) -> bool {
        self.edited.contains(&field)
    }

    pub fn is_empty(&self) -> bool {
        self.edited.is_empty() && self.meta.links.is_empty() && self.hidden_links.is_empty()
    }

    /// Adds another set of edits (of a copy combined into this entry); what is already here wins.
    pub fn merge(&mut self, other: UserEdits) {
        for field in &other.edited {
            if !self.is_edited(*field) {
                field.copy(&other.meta, &mut self.meta);
                self.edited.push(*field);
            }
        }
        for link in other.meta.links {
            if !self
                .meta
                .links
                .iter()
                .any(|l| url_key(&l.url) == url_key(&link.url))
            {
                self.meta.links.push(link);
            }
        }
        for url in other.hidden_links {
            if !self.hidden_links.contains(&url) {
                self.hidden_links.push(url);
            }
        }
    }
}

/// A link as the Edit dialog lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EditLink {
    pub link: GameLink,
    /// Id of the source it came from (`user` for the user's own links).
    pub source: String,
    pub hidden: bool,
}

/// Everything the Edit dialog needs beyond the game itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameEditInfo {
    pub edits: UserEdits,
    /// The values without the user's edits, and where each came from (field key -> source id):
    /// what "Use IGDB value" goes back to.
    pub automatic: GameMetadata,
    pub automatic_sources: BTreeMap<String, String>,
    /// Every source's links in priority order, the user's own first, hidden ones included.
    pub links: Vec<EditLink>,
    /// The sorting name made from the name, shown when the user has none of their own.
    pub automatic_sorting_name: String,
    /// The user's own sorting name.
    pub sorting_name: Option<String>,
    /// Which images the user picked themselves (they survive metadata refreshes).
    pub user_images: UserImages,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UserImages {
    pub cover: bool,
    pub background: bool,
    pub icon: bool,
}

/// What the Edit dialog saves besides the fields of [`super::GameUpdate`] (images and profiles
/// apply at once). The dialog always sends all of it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GameEdits {
    pub edits: UserEdits,
    /// The user's own sorting name; `None` or blank uses the automatic one.
    pub sorting_name: Option<String>,
    pub user_score: Option<u8>,
    pub notes: Option<String>,
}

/// One way to start a library entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LaunchProfile {
    /// For a store copy's own profile: the copy's game id; otherwise the profile's own id.
    pub id: Uuid,
    /// The shown name: the user's or, for a copy's profile, the store's name.
    pub name: String,
    /// The user's name for it, if any.
    pub custom_name: Option<String>,
    /// The store copy it starts (with `args` added), `None` for an executable profile.
    pub copy_id: Option<Uuid>,
    /// The copy's store id (`steam`, ...), for its icon.
    pub source: Option<String>,
    /// Profiles a store copy brings can't be deleted.
    pub own: bool,
    /// Extra command line arguments for a copy's launch.
    pub args: Option<String>,
    /// The executable to start, for executable profiles.
    pub action: Option<PlayAction>,
    /// Whether the copy's launch can take extra arguments.
    pub args_supported: bool,
    /// Whether it can be played now (its copy is installed; executable profiles always).
    pub installed: bool,
}

/// An entry's launch profiles in the user's order and the chosen default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LaunchProfiles {
    pub profiles: Vec<LaunchProfile>,
    /// The user's default; `None` is "Automatic" (the first installed copy in play order).
    pub default_id: Option<Uuid>,
    /// The profile Play starts now.
    pub play_id: Option<Uuid>,
}

/// A new or changed profile. For an existing copy profile only `name` and `args` apply.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ProfileInput {
    pub name: Option<String>,
    pub copy_id: Option<Uuid>,
    pub args: Option<String>,
    pub action: Option<PlayAction>,
}

/// An image the picker offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageOption {
    /// Full image: an `https` URL or a local file path.
    pub url: String,
    /// Smaller preview to show in the grid (the image itself when `None`).
    pub thumbnail: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Shown under the image: "Steam · library cover", "steamgriddb.com", ...
    pub label: String,
}

/// A place the image picker can ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageSource {
    pub id: String,
    pub name: String,
}

/// Which image of a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageSlot {
    Cover,
    Background,
    Icon,
}
