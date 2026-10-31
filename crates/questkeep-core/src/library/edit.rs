//! The Edit game dialog: the user's own metadata, images and launch profiles.

use std::path::Path;

use uuid::Uuid;

use super::Library;
use crate::db::ProfileRow;
use crate::error::{CoreError, Result};
use crate::events::CoreEvent;
use crate::images::{
    IgdbLookup, ImageKind, Lookup, SquareFrom, icon_png_from_bytes, icon_png_from_rgba,
    image_extension, is_user_image, user_image_key,
};
use crate::model::{
    ActionKind, EditLink, Game, GameEditInfo, GameEdits, ImageOption, ImageSlot, ImageSource,
    LaunchProfile, LaunchProfiles, PlayAction, ProfileInput, USER_SOURCE, UserImages, resolve,
    sorting_name, url_key,
};
use crate::providers::ImageCandidate;

/// Icon size requested from Windows for executables and DLLs the user picks.
const PICKED_ICON_SIZE: u32 = 48;

fn kind_of(slot: ImageSlot) -> ImageKind {
    match slot {
        ImageSlot::Cover => ImageKind::Cover,
        ImageSlot::Background => ImageKind::Background,
        ImageSlot::Icon => ImageKind::Icon,
    }
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

impl Library {
    /// What the Edit dialog shows besides the game: the user's edits, the values without them
    /// and every link with its source.
    pub fn game_edit_info(&self, id: Uuid) -> Result<GameEditInfo> {
        let entry = self.db.entry_of(id)?;
        let game = self.db.get_entry(entry)?;
        let records = self.db.source_records(entry)?;
        let edits = self.db.user_edits(entry)?;
        let priority = self.resolver.priority();
        let automatic = resolve(&records, None, &priority, self.resolver.sources());
        // Links in the order the page shows them, each with its source.
        let order = priority.order_for(None, self.resolver.sources());
        let rank = |source: &str| {
            order
                .iter()
                .position(|s| s == source)
                .unwrap_or(order.len())
        };
        let mut sorted: Vec<_> = records.iter().collect();
        sorted.sort_by_key(|r| rank(&r.source));
        let hidden: Vec<String> = edits.hidden_links.iter().map(|u| url_key(u)).collect();
        let mut seen = std::collections::HashSet::new();
        let mut links = Vec::new();
        for link in &edits.meta.links {
            if seen.insert(url_key(&link.url)) {
                links.push(EditLink {
                    link: link.clone(),
                    source: USER_SOURCE.to_string(),
                    hidden: false,
                });
            }
        }
        for record in sorted {
            for link in &record.data.links {
                let key = url_key(&link.url);
                if !link.url.trim().is_empty() && seen.insert(key.clone()) {
                    links.push(EditLink {
                        link: link.clone(),
                        source: record.source.clone(),
                        hidden: hidden.contains(&key),
                    });
                }
            }
        }
        Ok(GameEditInfo {
            edits,
            automatic: automatic.data,
            automatic_sources: automatic.sources,
            links,
            automatic_sorting_name: sorting_name(&game.name),
            sorting_name: self.db.user_sorting_name(entry)?,
            user_images: UserImages {
                cover: is_user_image(&game, ImageKind::Cover),
                background: is_user_image(&game, ImageKind::Background),
                icon: is_user_image(&game, ImageKind::Icon),
            },
        })
    }

    /// Saves the dialog's metadata part: the user's edits (then the shown values are computed
    /// again), sorting name, score and notes.
    pub fn save_game_edits(&self, id: Uuid, edits: &GameEdits) -> Result<Game> {
        let entry = self.db.entry_of(id)?;
        let mut user = edits.edits.clone();
        user.edited.sort();
        user.edited.dedup();
        user.meta.links.retain(|l| !l.url.trim().is_empty());
        for link in &mut user.meta.links {
            link.url = link.url.trim().to_string();
            link.name = link.name.trim().to_string();
            if link.name.is_empty() {
                link.name = link.url.clone();
            }
            if !(link.url.starts_with("https://") || link.url.starts_with("http://")) {
                return Err(CoreError::Invalid(format!(
                    "\"{}\" isn't a web address (it must start with https://).",
                    link.url
                )));
            }
        }
        self.db.set_user_edits(entry, &user)?;
        self.resolver.resolve(entry)?;
        self.db
            .set_user_sorting_name(entry, edits.sorting_name.as_deref())?;
        self.db
            .set_user_score_and_notes(entry, edits.user_score, edits.notes.as_deref())?;
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![entry] });
        self.get_game(entry)
    }

    /// Sets an image the user chose: a local file or a picked image (`https` URL or a file the
    /// picker listed). Icons may also come from an .ico, .exe or .dll.
    pub fn set_user_image(&self, id: Uuid, slot: ImageSlot, source: &str) -> Result<Game> {
        let entry = self.db.entry_of(id)?;
        let kind = kind_of(slot);
        let source = source.trim();
        let bytes = if source.starts_with("https://") || source.starts_with("http://") {
            let http = crate::images::http_agent();
            crate::images::download(&http, source)
                .map(|(_, bytes)| bytes)
                .ok_or_else(|| CoreError::Network("The image could not be downloaded.".into()))?
        } else {
            let path = Path::new(source);
            if !path.is_file() {
                return Err(CoreError::Invalid(format!(
                    "The file '{source}' does not exist."
                )));
            }
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if kind == ImageKind::Icon && matches!(ext.as_str(), "exe" | "dll") {
                let icon = crate::platform::executable_icon(path, PICKED_ICON_SIZE)
                    .ok_or_else(|| CoreError::Invalid("That file has no icon.".into()))?;
                let png = icon_png_from_rgba(&icon, SquareFrom::Icon)
                    .ok_or_else(|| CoreError::Invalid("That file has no usable icon.".into()))?;
                return self.store_user_image(entry, kind, "png", &png);
            }
            let size = std::fs::metadata(path)?.len();
            if size > crate::images::MAX_IMAGE_BYTES {
                return Err(CoreError::Invalid("The image is larger than 20 MB.".into()));
            }
            std::fs::read(path)?
        };
        if kind == ImageKind::Icon {
            let png = icon_png_from_bytes(&bytes, SquareFrom::Icon)
                .or_else(|| icon_png_from_bytes(&bytes, SquareFrom::CoverTop))
                .ok_or_else(|| {
                    CoreError::Invalid("Use a PNG, JPG or ICO file (or an .exe) as icon.".into())
                })?;
            return self.store_user_image(entry, kind, "png", &png);
        }
        let ext = image_extension(&bytes).ok_or_else(|| {
            CoreError::Invalid("Unsupported image type. Use a JPG, PNG or WebP file.".into())
        })?;
        self.store_user_image(entry, kind, ext, &bytes)
    }

    fn store_user_image(
        &self,
        entry: Uuid,
        kind: ImageKind,
        ext: &str,
        bytes: &[u8],
    ) -> Result<Game> {
        let relative = self.images.store_image(kind, entry, ext, bytes)?;
        match kind {
            ImageKind::Cover => self.db.set_cover_path(entry, Some(&relative))?,
            ImageKind::Background => self.db.set_background_path(entry, Some(&relative))?,
            ImageKind::Icon => {
                self.db.set_icon_path(entry, Some(&relative))?;
                self.db
                    .set_extra_value(entry, crate::images::ICON_FROM_COVER_KEY, None)?;
            }
        }
        self.db.set_extra_value(
            entry,
            user_image_key(kind),
            Some(serde_json::Value::Bool(true)),
        )?;
        if kind == ImageKind::Cover {
            let game = self.db.get_game(entry)?;
            if let Err(err) = crate::images::drop_cover_icon(&self.db, &self.images, &game) {
                tracing::warn!(%err, %entry, "could not drop the icon made from the old cover");
            }
        }
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![entry] });
        self.get_game(entry)
    }

    /// Removes the cover or background for good (no source fills it again), or resets the
    /// icon so it is made again from the store or the executable.
    pub fn remove_user_image(&self, id: Uuid, slot: ImageSlot) -> Result<Game> {
        let entry = self.db.entry_of(id)?;
        let kind = kind_of(slot);
        self.images.remove_image(kind, entry)?;
        match kind {
            ImageKind::Cover => self.db.set_cover_path(entry, None)?,
            ImageKind::Background => self.db.set_background_path(entry, None)?,
            ImageKind::Icon => self.db.set_icon_path(entry, None)?,
        }
        let marker = (kind != ImageKind::Icon).then_some(serde_json::Value::Bool(true));
        self.db
            .set_extra_value(entry, user_image_key(kind), marker)?;
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![entry] });
        self.get_game(entry)
    }

    /// The places the image picker can ask for this game: its stores, IGDB and SteamGridDB
    /// when configured.
    pub fn image_sources(&self, id: Uuid, slot: ImageSlot) -> Result<Vec<ImageSource>> {
        let game = self.get_game(id)?;
        let kind = kind_of(slot);
        let mut sources: Vec<ImageSource> = Vec::new();
        for copy in &game.copies {
            let (Some(provider), Some(store_id)) = (
                self.providers.get(&copy.source),
                copy.source_game_id.as_deref(),
            ) else {
                continue;
            };
            if !store_candidates(provider.as_ref(), store_id, kind).is_empty()
                && !sources.iter().any(|s| s.id == copy.source)
            {
                sources.push(ImageSource {
                    id: copy.source.clone(),
                    name: provider.name().to_string(),
                });
            }
        }
        if self.igdb.is_configured() && kind != ImageKind::Icon {
            sources.push(ImageSource {
                id: crate::model::IGDB_SOURCE.into(),
                name: "IGDB".into(),
            });
        }
        if self.artwork.is_configured() {
            sources.push(ImageSource {
                id: "steamgriddb".into(),
                name: "SteamGridDB".into(),
            });
        }
        Ok(sources)
    }

    /// One source's images for the picker. Network sources may take a few seconds.
    pub fn image_options(
        &self,
        id: Uuid,
        slot: ImageSlot,
        source: &str,
    ) -> Result<Vec<ImageOption>> {
        let game = self.get_game(id)?;
        let kind = kind_of(slot);
        let year = game
            .release_date
            .as_deref()
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse().ok());
        match source {
            crate::model::IGDB_SOURCE => self.igdb.image_options(
                &IgdbLookup {
                    name: &game.name,
                    year,
                },
                kind,
            ),
            "steamgriddb" => {
                let copy = game
                    .copies
                    .iter()
                    .find(|c| crate::images::platform_name(&c.source).is_some());
                self.artwork.image_options(
                    &Lookup {
                        name: &game.name,
                        source: copy.map_or(game.source.as_str(), |c| c.source.as_str()),
                        source_game_id: copy.map_or(game.source_game_id.as_deref(), |c| {
                            c.source_game_id.as_deref()
                        }),
                    },
                    kind,
                )
            }
            store => {
                let Some(copy) = game.copies.iter().find(|c| c.source == store) else {
                    return Ok(Vec::new());
                };
                let (Some(provider), Some(store_id)) =
                    (self.providers.get(store), copy.source_game_id.as_deref())
                else {
                    return Ok(Vec::new());
                };
                let name = provider.name().to_string();
                Ok(store_candidates(provider.as_ref(), store_id, kind)
                    .into_iter()
                    .filter_map(|candidate| match candidate {
                        ImageCandidate::Url(url) => Some(url),
                        ImageCandidate::File(path) => {
                            path.is_file().then(|| path.display().to_string())
                        }
                    })
                    .map(|url| ImageOption {
                        label: format!("{name} · {}", image_name(&url)),
                        thumbnail: None,
                        width: None,
                        height: None,
                        url,
                    })
                    .collect())
            }
        }
    }

    /// The entry's launch profiles: one per store copy (in play order, unless the user moved
    /// them) and the user's own.
    pub fn launch_profiles(&self, id: Uuid) -> Result<LaunchProfiles> {
        let entry = self.db.entry_of(id)?;
        let game = self.db.get_entry(entry)?;
        let rows = self.db.profile_rows(entry)?;
        let mut listed: Vec<(i64, LaunchProfile)> = Vec::new();
        for (index, copy) in game.copies.iter().enumerate() {
            let row = rows
                .iter()
                .find(|r| r.id == copy.id && r.entry_id.is_none());
            let provider = self.providers.get(&copy.source);
            let store_name = provider
                .as_ref()
                .map_or_else(|| copy.source.clone(), |p| p.name().to_string());
            let base = if copy.source == crate::model::MANUAL_SOURCE {
                "Executable".to_string()
            } else {
                store_name
            };
            let custom_name = row.and_then(|r| r.name.clone());
            let position = row.map_or(10_000 + index as i64, |r| r.position);
            listed.push((
                position,
                LaunchProfile {
                    id: copy.id,
                    name: custom_name.clone().unwrap_or(base),
                    custom_name,
                    copy_id: Some(copy.id),
                    source: Some(copy.source.clone()),
                    own: false,
                    args: row.and_then(|r| r.args.clone()),
                    action: None,
                    args_supported: self.args_supported(copy.id),
                    installed: copy.installed,
                },
            ));
        }
        for row in rows.iter().filter(|r| r.entry_id.is_some()) {
            let copy = row
                .copy_id
                .and_then(|c| game.copies.iter().find(|copy| copy.id == c));
            if row.copy_id.is_some() && copy.is_none() {
                continue; // its copy left the entry
            }
            listed.push((
                row.position,
                LaunchProfile {
                    id: row.id,
                    name: row.name.clone().unwrap_or_else(|| "Profile".into()),
                    custom_name: row.name.clone(),
                    copy_id: row.copy_id,
                    source: copy.map(|c| c.source.clone()),
                    own: true,
                    args: row.args.clone(),
                    action: row.action.clone(),
                    args_supported: copy.is_none_or(|c| self.args_supported(c.id)),
                    installed: copy.is_none_or(|c| c.installed),
                },
            ));
        }
        listed.sort_by_key(|(position, _)| *position);
        let profiles: Vec<LaunchProfile> = listed.into_iter().map(|(_, p)| p).collect();
        let default_id = self
            .db
            .default_profile(entry)?
            .filter(|d| profiles.iter().any(|p| p.id == *d));
        let play_id = default_id.or_else(|| {
            game.copies
                .iter()
                .find(|c| c.installed)
                .or(game.copies.first())
                .map(|c| c.id)
        });
        Ok(LaunchProfiles {
            profiles,
            default_id,
            play_id,
        })
    }

    /// Whether a copy's launch can take extra arguments: executables always, store URIs when
    /// the store supports it.
    fn args_supported(&self, copy: Uuid) -> bool {
        let Ok(game) = self.db.get_game(copy) else {
            return false;
        };
        match &game.play_action {
            Some(action) if action.kind == ActionKind::File => true,
            _ => match (
                self.providers.get(&game.source),
                game.source_game_id.as_deref(),
            ) {
                (Some(provider), Some(id)) => provider.launch_with_args(id, "-x").is_some(),
                _ => false,
            },
        }
    }

    /// Adds a profile (`profile` `None`) or changes one. A copy's own profile only takes a name
    /// and arguments.
    pub fn save_launch_profile(
        &self,
        id: Uuid,
        profile: Option<Uuid>,
        input: &ProfileInput,
    ) -> Result<LaunchProfiles> {
        let entry = self.db.entry_of(id)?;
        let current = self.launch_profiles(entry)?;
        let name = trimmed(input.name.as_deref());
        let args = trimmed(input.args.as_deref());
        let next_position = current.profiles.len() as i64;
        let row = match profile.and_then(|p| current.profiles.iter().find(|x| x.id == p)) {
            Some(existing) if !existing.own => ProfileRow {
                id: existing.id,
                entry_id: None,
                copy_id: existing.copy_id,
                name,
                args,
                action: None,
                position: self.position_of(&current, existing.id),
            },
            Some(existing) => ProfileRow {
                id: existing.id,
                entry_id: Some(entry),
                copy_id: input.copy_id,
                name,
                args,
                action: input.action.clone(),
                position: self.position_of(&current, existing.id),
            },
            None if profile.is_some() => {
                return Err(CoreError::Invalid("That launch profile is gone.".into()));
            }
            None => ProfileRow {
                id: Uuid::new_v4(),
                entry_id: Some(entry),
                copy_id: input.copy_id,
                name: name.or_else(|| Some(format!("Profile {}", next_position + 1))),
                args,
                action: input.action.clone(),
                position: next_position,
            },
        };
        if row.entry_id.is_some() {
            match (row.copy_id, &row.action) {
                (Some(copy), _) => {
                    if !current
                        .profiles
                        .iter()
                        .any(|p| p.copy_id == Some(copy) && !p.own)
                    {
                        return Err(CoreError::Invalid(
                            "That store's copy isn't part of this game.".into(),
                        ));
                    }
                }
                (None, Some(action)) if !action.target.trim().is_empty() => {}
                (None, _) => {
                    return Err(CoreError::Invalid(
                        "Choose the executable this profile starts.".into(),
                    ));
                }
            }
        }
        if row.args.is_some()
            && let Some(copy) = row.copy_id
            && !self.args_supported(copy)
        {
            return Err(CoreError::Invalid(
                "This store can't pass arguments; base the profile on the executable instead."
                    .into(),
            ));
        }
        self.db.save_profile(&row)?;
        self.profiles_changed(entry)
    }

    /// Where a profile is in the list now, written as its stored position.
    fn position_of(&self, current: &LaunchProfiles, id: Uuid) -> i64 {
        current
            .profiles
            .iter()
            .position(|p| p.id == id)
            .unwrap_or(current.profiles.len()) as i64
    }

    /// Deletes an own profile, or drops the changes of a copy's profile.
    pub fn delete_launch_profile(&self, id: Uuid, profile: Uuid) -> Result<LaunchProfiles> {
        let entry = self.db.entry_of(id)?;
        let current = self.launch_profiles(entry)?;
        if !current.profiles.iter().any(|p| p.id == profile) {
            return Err(CoreError::Invalid("That launch profile is gone.".into()));
        }
        self.db.delete_profile(profile)?;
        self.profiles_changed(entry)
    }

    /// Puts the profiles in the given order (ids not listed keep their place after them).
    pub fn order_launch_profiles(&self, id: Uuid, order: &[Uuid]) -> Result<LaunchProfiles> {
        let entry = self.db.entry_of(id)?;
        let current = self.launch_profiles(entry)?;
        let mut ids: Vec<Uuid> = order
            .iter()
            .filter(|id| current.profiles.iter().any(|p| p.id == **id))
            .copied()
            .collect();
        ids.extend(
            current
                .profiles
                .iter()
                .map(|p| p.id)
                .filter(|id| !order.contains(id)),
        );
        let rows = self.db.profile_rows(entry)?;
        for (position, pid) in ids.iter().enumerate() {
            let Some(profile) = current.profiles.iter().find(|p| p.id == *pid) else {
                continue;
            };
            let row = rows
                .iter()
                .find(|r| r.id == *pid)
                .cloned()
                .unwrap_or(ProfileRow {
                    id: profile.id,
                    entry_id: None,
                    copy_id: profile.copy_id,
                    name: None,
                    args: None,
                    action: None,
                    position: 0,
                });
            self.db.save_profile(&ProfileRow {
                position: position as i64,
                ..row
            })?;
        }
        self.profiles_changed(entry)
    }

    /// Makes a profile the default (`None`: back to automatic).
    pub fn set_default_launch_profile(
        &self,
        id: Uuid,
        profile: Option<Uuid>,
    ) -> Result<LaunchProfiles> {
        let entry = self.db.entry_of(id)?;
        if let Some(profile) = profile
            && !self
                .launch_profiles(entry)?
                .profiles
                .iter()
                .any(|p| p.id == profile)
        {
            return Err(CoreError::Invalid("That launch profile is gone.".into()));
        }
        self.db.set_default_profile(entry, profile)?;
        self.profiles_changed(entry)
    }

    fn profiles_changed(&self, entry: Uuid) -> Result<LaunchProfiles> {
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![entry] });
        self.launch_profiles(entry)
    }

    /// The game row to launch for a profile (`None`: the default, else automatic), with the
    /// profile's launch in its play action.
    pub fn launch_target(&self, id: Uuid, profile: Option<Uuid>) -> Result<Game> {
        let entry = self.db.entry_of(id)?;
        let profiles = self.launch_profiles(entry)?;
        let chosen = match profile {
            Some(p) => Some(
                profiles
                    .profiles
                    .iter()
                    .find(|x| x.id == p)
                    .ok_or_else(|| CoreError::Invalid("That launch profile is gone.".into()))?,
            ),
            None => profiles
                .default_id
                .and_then(|d| profiles.profiles.iter().find(|x| x.id == d)),
        };
        let Some(chosen) = chosen else {
            return self.play_target(entry, None);
        };
        match (chosen.copy_id, &chosen.action) {
            (Some(copy), _) => {
                let mut game = self.db.get_game(copy)?;
                if let Some(args) = chosen.args.as_deref().filter(|a| !a.trim().is_empty()) {
                    game.play_action = Some(self.with_args(&game, args)?);
                }
                Ok(game)
            }
            (None, Some(action)) => {
                let mut game = self.db.get_game(entry)?;
                game.install_dir = Path::new(&action.target)
                    .parent()
                    .map(|p| p.display().to_string());
                game.play_action = Some(action.clone());
                Ok(game)
            }
            (None, None) => self.play_target(entry, None),
        }
    }

    /// A copy's launch with extra arguments.
    fn with_args(&self, game: &Game, args: &str) -> Result<PlayAction> {
        let action = game.play_action.clone().ok_or_else(|| {
            CoreError::Launch(format!("{} has no launch action configured.", game.name))
        })?;
        if action.kind == ActionKind::File {
            let args = match action
                .args
                .as_deref()
                .map(str::trim)
                .filter(|a| !a.is_empty())
            {
                Some(own) => format!("{own} {args}"),
                None => args.to_string(),
            };
            return Ok(PlayAction {
                args: Some(args),
                ..action
            });
        }
        let launch = game
            .source_game_id
            .as_deref()
            .zip(self.providers.get(&game.source))
            .and_then(|(id, provider)| provider.launch_with_args(id, args))
            .ok_or_else(|| {
                CoreError::Launch(
                    "This store can't pass arguments; base the profile on the executable instead."
                        .into(),
                )
            })?;
        Ok(PlayAction {
            tracking: action.tracking,
            tracking_path: action.tracking_path,
            ..launch
        })
    }
}

/// A store's image candidates of one kind.
fn store_candidates(
    provider: &dyn crate::providers::LibraryProvider,
    store_id: &str,
    kind: ImageKind,
) -> Vec<ImageCandidate> {
    match kind {
        ImageKind::Cover => provider.cover_candidates(store_id),
        ImageKind::Background => provider.background_candidates(store_id),
        ImageKind::Icon => provider.icon_candidates(store_id),
    }
}

/// A short name for an image from its file name: "library_600x900.jpg" -> "library 600x900".
fn image_name(url: &str) -> String {
    let file = url
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(url)
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    stem.replace(['_', '-'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GameLink, GameMetadata, MetadataField, NewManualGame, UserEdits};

    fn library() -> (tempfile::TempDir, Library, Uuid) {
        let (tmp, lib) = super::super::tests::test_library();
        let exe = tmp.path().join("game.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let game = lib
            .add_manual_game(&NewManualGame {
                name: Some("Hades".into()),
                executable: exe.display().to_string(),
                args: None,
                working_dir: None,
                cover_image: None,
            })
            .unwrap();
        (tmp, lib, game.id)
    }

    #[test]
    fn edits_survive_and_go_back_to_automatic() {
        let (_tmp, lib, id) = library();
        let igdb = GameMetadata {
            description: Some("From IGDB".into()),
            tags: vec!["Roguelike".into()],
            links: vec![GameLink::new("Wiki", "https://wiki.example")],
            ..GameMetadata::default()
        };
        lib.db
            .set_source_metadata(id, "igdb", Some(&igdb), chrono::Utc::now())
            .unwrap();
        lib.resolver.resolve(id).unwrap();
        let mut edits = UserEdits::default();
        edits.meta.description = Some("Mine".into());
        edits.edited = vec![MetadataField::Description, MetadataField::Tags];
        edits.hidden_links = vec!["https://wiki.example".into()];
        edits.meta.links = vec![GameLink::new("", "https://mods.example")];
        let game = lib
            .save_game_edits(
                id,
                &GameEdits {
                    edits,
                    sorting_name: Some("Hades 1".into()),
                    user_score: Some(90),
                    notes: Some(" good ".into()),
                },
            )
            .unwrap();
        assert_eq!(game.description.as_deref(), Some("Mine"));
        assert!(game.tags.is_empty());
        assert_eq!(game.links.len(), 1);
        assert_eq!(game.links[0].name, "https://mods.example");
        assert_eq!(game.sorting_name, "Hades 1");
        assert_eq!(game.user_score, Some(90));
        assert_eq!(game.notes.as_deref(), Some("good"));

        // A refresh of IGDB doesn't touch the edit.
        lib.db
            .set_source_metadata(id, "igdb", Some(&igdb), chrono::Utc::now())
            .unwrap();
        lib.resolver.resolve(id).unwrap();
        let info = lib.game_edit_info(id).unwrap();
        assert_eq!(
            lib.get_game(id).unwrap().description.as_deref(),
            Some("Mine")
        );
        assert_eq!(info.automatic.description.as_deref(), Some("From IGDB"));
        assert_eq!(info.automatic_sources["description"], "igdb");
        assert_eq!(info.automatic_sorting_name, "Hades");
        assert_eq!(info.sorting_name.as_deref(), Some("Hades 1"));
        assert_eq!(
            info.links
                .iter()
                .map(|l| (l.source.as_str(), l.hidden))
                .collect::<Vec<_>>(),
            [("user", false), ("igdb", true)]
        );

        // Dropping the edits brings the automatic values back.
        let game = lib.save_game_edits(id, &GameEdits::default()).unwrap();
        assert_eq!(game.description.as_deref(), Some("From IGDB"));
        assert_eq!(game.tags, ["Roguelike"]);
        assert_eq!(game.sorting_name, "Hades");
        assert_eq!(game.user_score, None);
    }

    #[test]
    fn profiles_default_to_automatic_and_launch_with_arguments() {
        let (tmp, lib, id) = library();
        let profiles = lib.launch_profiles(id).unwrap();
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(profiles.profiles[0].name, "Executable");
        assert!(profiles.profiles[0].args_supported);
        assert_eq!(profiles.default_id, None);
        assert_eq!(profiles.play_id, Some(id));

        // Extra arguments on the copy's profile.
        let profiles = lib
            .save_launch_profile(
                id,
                Some(id),
                &ProfileInput {
                    args: Some("-windowed".into()),
                    ..ProfileInput::default()
                },
            )
            .unwrap();
        assert_eq!(profiles.profiles[0].args.as_deref(), Some("-windowed"));
        let target = lib.launch_target(id, None).unwrap();
        assert_eq!(
            target.play_action.unwrap().args,
            None,
            "automatic plays as before"
        );
        let target = lib.launch_target(id, Some(id)).unwrap();
        assert_eq!(
            target.play_action.unwrap().args.as_deref(),
            Some("-windowed")
        );

        // An own executable profile made the default, moved first, then deleted.
        let tool = tmp.path().join("tool.exe");
        std::fs::write(&tool, b"MZ").unwrap();
        let profiles = lib
            .save_launch_profile(
                id,
                None,
                &ProfileInput {
                    name: Some("Editor".into()),
                    action: Some(PlayAction::file(tool.display().to_string())),
                    ..ProfileInput::default()
                },
            )
            .unwrap();
        let editor = profiles.profiles[1].id;
        assert!(profiles.profiles[1].own);
        let profiles = lib.set_default_launch_profile(id, Some(editor)).unwrap();
        assert_eq!(profiles.play_id, Some(editor));
        let target = lib.launch_target(id, None).unwrap();
        assert_eq!(
            target.play_action.unwrap().target,
            tool.display().to_string()
        );
        let profiles = lib.order_launch_profiles(id, &[editor]).unwrap();
        assert_eq!(profiles.profiles[0].id, editor);
        let profiles = lib.delete_launch_profile(id, editor).unwrap();
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(profiles.default_id, None, "the default went with it");
        assert!(
            lib.save_launch_profile(id, None, &ProfileInput::default())
                .is_err(),
            "an own profile needs an executable or a copy"
        );
    }

    #[test]
    fn user_images_are_kept_and_removed_for_good() {
        let (tmp, lib, id) = library();
        let png = tmp.path().join("cover.png");
        image::RgbImage::new(60, 90).save(&png).unwrap();
        let game = lib
            .set_user_image(id, ImageSlot::Cover, &png.display().to_string())
            .unwrap();
        assert!(game.cover_path.is_some());
        assert!(lib.game_edit_info(id).unwrap().user_images.cover);
        let game = lib.remove_user_image(id, ImageSlot::Cover).unwrap();
        assert!(game.cover_path.is_none());
        assert!(
            lib.game_edit_info(id).unwrap().user_images.cover,
            "removed stays removed"
        );
        let icon = tmp.path().join("icon.png");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([200, 10, 10, 255]))
            .save(&icon)
            .unwrap();
        let game = lib
            .set_user_image(id, ImageSlot::Icon, &icon.display().to_string())
            .unwrap();
        assert!(game.icon_path.is_some());
        lib.remove_user_image(id, ImageSlot::Icon).unwrap();
        assert!(!lib.game_edit_info(id).unwrap().user_images.icon, "reset");
        assert_eq!(
            image_name("https://x/apps/1/library_600x900.jpg?t=1"),
            "library 600x900"
        );
    }
}
