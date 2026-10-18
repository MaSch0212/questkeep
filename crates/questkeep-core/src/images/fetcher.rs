//! Background image and metadata fetching.
//!
//! Covers: a fixed number of worker threads pull jobs from a channel, so at most `workers`
//! downloads run at once no matter how many games are queued. Each job tries the provider's cover
//! candidates in order (local files before URLs) and stores the first valid image.
//!
//! Metadata: a single worker calls store APIs one game at a time with a minimum interval, so we
//! stay well below store rate limits, then fetches a background image if the game has none.
//!
//! Icons (small square PNGs for the tray menu) are produced by the same worker, after metadata and
//! background, or on demand through [`MetadataFetcher::enqueue_icons`] for games the tray lists.

use std::collections::HashSet;
use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use chrono::Utc;
use uuid::Uuid;

use super::icons::{SquareFrom, icon_png_from_bytes, icon_png_from_rgba};
use super::steamgriddb::{Lookup, SteamGridDb, checked_key};
use super::{ImageCache, ImageKind, MAX_IMAGE_BYTES};
use crate::db::Database;
use crate::events::{CoreEvent, EventBus};
use crate::model::{ActionKind, Game};
use crate::providers::{ImageCandidate, LibraryProvider, ProviderRegistry};

/// One game whose cover should be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverJob {
    pub game_id: Uuid,
    pub source: String,
    /// `None` for manual games and other games without a store id; those only get SteamGridDB.
    pub source_game_id: Option<String>,
    /// Replace existing artwork with SteamGridDB's (cover and background), ignoring the
    /// provider, the "checked" marker and the once-per-run limit. Used for manual re-fetching.
    pub force: bool,
}

struct Shared {
    db: Arc<Database>,
    images: ImageCache,
    providers: ProviderRegistry,
    events: EventBus,
    artwork: SteamGridDb,
    http: ureq::Agent,
    /// Games already attempted in this run, so failures aren't retried in a loop.
    attempted: Mutex<HashSet<Uuid>>,
}

/// Owns the worker threads. Dropping it closes the queue; workers finish their current job and
/// exit. They are not joined: a half-finished download only leaves a `.part` file behind.
pub struct CoverFetcher {
    sender: Mutex<Option<Sender<CoverJob>>>,
    shared: Arc<Shared>,
}

impl CoverFetcher {
    pub fn new(
        db: Arc<Database>,
        images: ImageCache,
        providers: ProviderRegistry,
        events: EventBus,
        artwork: SteamGridDb,
        workers: usize,
    ) -> Self {
        let http = http_agent();
        let shared = Arc::new(Shared {
            db,
            images,
            providers,
            events,
            artwork,
            http,
            attempted: Mutex::new(HashSet::new()),
        });
        let (sender, receiver) = mpsc::channel::<CoverJob>();
        let receiver = Arc::new(Mutex::new(receiver));
        for i in 0..workers.max(1) {
            let shared = shared.clone();
            let receiver = receiver.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("cover-fetch-{i}"))
                .spawn(move || worker(&shared, &receiver));
            if let Err(err) = spawned {
                tracing::error!(%err, "could not spawn cover worker");
            }
        }
        Self {
            sender: Mutex::new(Some(sender)),
            shared,
        }
    }

    /// Queues jobs, skipping games already attempted in this run.
    pub fn enqueue(&self, jobs: impl IntoIterator<Item = CoverJob>) -> usize {
        let guard = self.sender.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(sender) = guard.as_ref() else {
            return 0;
        };
        let mut attempted = self
            .shared
            .attempted
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut queued = 0;
        for job in jobs {
            if (attempted.insert(job.game_id) || job.force) && sender.send(job).is_ok() {
                queued += 1;
            }
        }
        queued
    }

    /// Stops accepting jobs; workers exit after their current job.
    pub fn close(&self) {
        self.sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

impl Drop for CoverFetcher {
    fn drop(&mut self) {
        self.close();
    }
}

fn worker(shared: &Shared, receiver: &Mutex<Receiver<CoverJob>>) {
    loop {
        // Hold the lock only while waiting for the next job.
        let job = {
            let rx = receiver.lock().unwrap_or_else(PoisonError::into_inner);
            rx.recv()
        };
        let Ok(job) = job else {
            return; // queue closed
        };
        process(shared, &job);
    }
}

fn process(shared: &Shared, job: &CoverJob) {
    let Ok(Some(game)) = shared.db.find_game(job.game_id) else {
        return;
    };
    if game.cover_path.is_some() && !job.force {
        return;
    }
    let provider = job
        .source_game_id
        .as_deref()
        .and_then(|_| shared.providers.get(&job.source));
    let sgdb = |kind| steamgriddb_image(shared, &game, kind);
    let sgdb_wanted =
        shared.artwork.is_configured() && (job.force || !is_checked(&game, ImageKind::Cover));

    let found = if job.force {
        sgdb(ImageKind::Cover)
    } else {
        let from_provider = || {
            let provider = provider.as_ref()?;
            let id = job.source_game_id.as_deref()?;
            first_image(&shared.http, &provider.cover_candidates(id))
        };
        let low_quality = provider
            .as_ref()
            .is_some_and(|p| p.covers_are_low_quality());
        if low_quality && sgdb_wanted {
            // Square logos and the like: real box art first, the logo only as a fallback.
            sgdb(ImageKind::Cover).or_else(from_provider)
        } else {
            from_provider().or_else(|| sgdb_wanted.then(|| sgdb(ImageKind::Cover)).flatten())
        }
    };
    let mut changed = match found {
        Some((ext, bytes)) => store(shared, &game, ImageKind::Cover, ext, &bytes),
        None => {
            tracing::debug!(game = %job.game_id, "no cover found");
            false
        }
    };

    // Games with a store background get it from the metadata worker, which also falls back to
    // SteamGridDB itself. Games without a provider never reach that worker and providers without
    // any background source would only get it on first metadata fetch, so do it here.
    let provider_has_background = || {
        let (Some(provider), Some(id)) = (provider.as_ref(), job.source_game_id.as_deref()) else {
            return false;
        };
        !provider.background_candidates(id).is_empty()
    };
    let wants_background = shared.artwork.is_configured()
        && (job.force
            || (game.background_path.is_none()
                && !is_checked(&game, ImageKind::Background)
                && !provider_has_background()));
    if wants_background && let Some((ext, bytes)) = sgdb(ImageKind::Background) {
        changed |= store(shared, &game, ImageKind::Background, ext, &bytes);
    }
    if changed {
        shared.events.publish(CoreEvent::GamesChanged {
            ids: vec![job.game_id],
        });
    }
}

/// Stores a downloaded image and updates the game row. The game may have been removed
/// meanwhile, which is only logged.
fn store(shared: &Shared, game: &Game, kind: ImageKind, ext: &str, bytes: &[u8]) -> bool {
    shared
        .images
        .store_image(kind, game.id, ext, bytes)
        .and_then(|rel| match kind {
            ImageKind::Cover => shared.db.set_cover_path(game.id, Some(&rel)),
            ImageKind::Background => shared.db.set_background_path(game.id, Some(&rel)),
            ImageKind::Icon => shared.db.set_icon_path(game.id, Some(&rel)),
        })
        .inspect_err(|err| tracing::warn!(game = %game.id, %err, "could not store image"))
        .is_ok()
}

fn is_checked(game: &Game, kind: ImageKind) -> bool {
    game.extra.get(checked_key(kind)).is_some()
}

/// Asks SteamGridDB for the game's cover or background. "Nothing found" is remembered in
/// `extra`; errors (offline, rate limit, bad key) are only logged so they are retried later.
fn steamgriddb_image(
    shared: &Shared,
    game: &Game,
    kind: ImageKind,
) -> Option<(&'static str, Vec<u8>)> {
    steamgriddb_lookup(&shared.db, &shared.artwork, &shared.http, game, kind)
}

fn steamgriddb_lookup(
    db: &Database,
    artwork: &SteamGridDb,
    http: &ureq::Agent,
    game: &Game,
    kind: ImageKind,
) -> Option<(&'static str, Vec<u8>)> {
    let lookup = Lookup {
        name: &game.name,
        source: &game.source,
        source_game_id: game.source_game_id.as_deref(),
    };
    match artwork.fetch_image(http, &lookup, kind) {
        Ok(Some(image)) => Some(image),
        Ok(None) => {
            let marker = serde_json::Value::String(Utc::now().to_rfc3339());
            if let Err(err) = db.set_extra_value(game.id, checked_key(kind), Some(marker)) {
                tracing::warn!(game = %game.id, %err, "could not remember the SteamGridDB miss");
            }
            None
        }
        Err(err) => {
            tracing::debug!(game = %game.id, %err, "SteamGridDB lookup failed");
            None
        }
    }
}

/// Icon size requested from Windows for executables; the stored icon is scaled down from it.
const EXECUTABLE_ICON_SIZE: u32 = 48;

pub(crate) fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(concat!("Questkeep/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Loads the first candidate that yields a valid image.
fn first_image(
    http: &ureq::Agent,
    candidates: &[ImageCandidate],
) -> Option<(&'static str, Vec<u8>)> {
    candidates.iter().find_map(|candidate| match candidate {
        ImageCandidate::File(path) => load_file(path),
        ImageCandidate::Url(url) => download(http, url),
    })
}

/// One game whose store metadata (and background) should be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataJob {
    pub game_id: Uuid,
    pub source: String,
    pub source_game_id: String,
}

struct MetadataShared {
    db: Arc<Database>,
    images: ImageCache,
    providers: ProviderRegistry,
    events: EventBus,
    artwork: SteamGridDb,
    http: ureq::Agent,
    interval: Duration,
    /// Games waiting in the queue, so repeated enqueues don't pile up duplicates.
    queued: Mutex<HashSet<Uuid>>,
    /// Games whose icon is queued or was already attempted in this run. A failed attempt for a
    /// game without a cover is forgotten, so it is retried once the cover exists.
    icons_seen: Mutex<HashSet<Uuid>>,
}

/// Work of the metadata worker.
enum Job {
    Metadata(MetadataJob),
    /// Make the game's tray icon.
    Icon(Uuid),
}

/// Single-worker metadata queue. Like [`CoverFetcher`], closing it stops the worker after its
/// current job.
pub struct MetadataFetcher {
    sender: Mutex<Option<Sender<Job>>>,
    shared: Arc<MetadataShared>,
}

impl MetadataFetcher {
    /// `interval` is the minimum time between two store requests.
    pub fn new(
        db: Arc<Database>,
        images: ImageCache,
        providers: ProviderRegistry,
        events: EventBus,
        artwork: SteamGridDb,
        interval: Duration,
    ) -> Self {
        let shared = Arc::new(MetadataShared {
            db,
            images,
            providers,
            events,
            artwork,
            http: http_agent(),
            interval,
            queued: Mutex::new(HashSet::new()),
            icons_seen: Mutex::new(HashSet::new()),
        });
        let (sender, receiver) = mpsc::channel::<Job>();
        let worker_shared = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("metadata-fetch".into())
            .spawn(move || metadata_worker(&worker_shared, &receiver));
        if let Err(err) = spawned {
            tracing::error!(%err, "could not spawn metadata worker");
        }
        Self {
            sender: Mutex::new(Some(sender)),
            shared,
        }
    }

    /// Queues jobs, skipping games already waiting. Returns how many were queued.
    pub fn enqueue(&self, jobs: impl IntoIterator<Item = MetadataJob>) -> usize {
        let guard = self.sender.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(sender) = guard.as_ref() else {
            return 0;
        };
        let mut queued = self
            .shared
            .queued
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut count = 0;
        for job in jobs {
            if queued.insert(job.game_id) && sender.send(Job::Metadata(job)).is_ok() {
                count += 1;
            }
        }
        count
    }

    /// Queues icon jobs for games that lack an icon, skipping games already queued or attempted
    /// in this run. Returns how many were queued. Cheap enough for the tray to call whenever it
    /// lists a game without an icon; the work happens on the worker thread.
    pub fn enqueue_icons(&self, ids: impl IntoIterator<Item = Uuid>) -> usize {
        let guard = self.sender.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(sender) = guard.as_ref() else {
            return 0;
        };
        let mut seen = self
            .shared
            .icons_seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut count = 0;
        for id in ids {
            if seen.insert(id) && sender.send(Job::Icon(id)).is_ok() {
                count += 1;
            }
        }
        count
    }

    /// Stops accepting jobs; the worker exits after its current job.
    pub fn close(&self) {
        self.sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

impl Drop for MetadataFetcher {
    fn drop(&mut self) {
        self.close();
    }
}

fn metadata_worker(shared: &MetadataShared, receiver: &Receiver<Job>) {
    let mut last_request: Option<Instant> = None;
    while let Ok(job) = receiver.recv() {
        match job {
            Job::Metadata(job) => process_metadata(shared, &job, &mut last_request),
            Job::Icon(id) => {
                if fetch_icon(shared, id) {
                    shared
                        .events
                        .publish(CoreEvent::GamesChanged { ids: vec![id] });
                }
            }
        }
    }
}

fn process_metadata(
    shared: &MetadataShared,
    job: &MetadataJob,
    last_request: &mut Option<Instant>,
) {
    if let Some(provider) = shared.providers.get(&job.source) {
        if let Some(elapsed) = last_request.map(|t| t.elapsed())
            && elapsed < shared.interval
        {
            std::thread::sleep(shared.interval - elapsed);
        }
        *last_request = Some(Instant::now());
        let mut changed = match provider.metadata(&job.source_game_id) {
            Ok(Some(meta)) => shared
                .db
                .set_metadata(job.game_id, &meta, Utc::now())
                .is_ok(),
            Ok(None) => {
                if let Err(err) = shared.db.mark_metadata_checked(job.game_id, Utc::now()) {
                    tracing::warn!(game = %job.game_id, %err, "could not mark metadata");
                }
                false
            }
            // Not marked as checked, so it is retried on the next start.
            Err(err) => {
                tracing::debug!(game = %job.game_id, %err, "metadata fetch failed");
                false
            }
        };
        changed |= fetch_background(shared, provider.as_ref(), job);
        // Last step: the store data is cached by now, so this adds no request for most stores.
        shared
            .icons_seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(job.game_id);
        changed |= fetch_icon(shared, job.game_id);
        if changed {
            shared.events.publish(CoreEvent::GamesChanged {
                ids: vec![job.game_id],
            });
        }
    }
    shared
        .queued
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&job.game_id);
}

/// Stores a background if the game has none yet. Returns whether one was stored.
fn fetch_background(
    shared: &MetadataShared,
    provider: &dyn LibraryProvider,
    job: &MetadataJob,
) -> bool {
    let Ok(Some(game)) = shared.db.find_game(job.game_id) else {
        return false;
    };
    if game.background_path.is_some() {
        return false;
    }
    let candidates = provider.background_candidates(&job.source_game_id);
    let found = first_image(&shared.http, &candidates).or_else(|| {
        (shared.artwork.is_configured() && !is_checked(&game, ImageKind::Background))
            .then(|| {
                steamgriddb_lookup(
                    &shared.db,
                    &shared.artwork,
                    &shared.http,
                    &game,
                    ImageKind::Background,
                )
            })
            .flatten()
    });
    let Some((ext, bytes)) = found else {
        return false;
    };
    shared
        .images
        .store_image(ImageKind::Background, job.game_id, ext, &bytes)
        .and_then(|rel| shared.db.set_background_path(job.game_id, Some(&rel)))
        .inspect_err(|err| tracing::warn!(game = %job.game_id, %err, "could not store background"))
        .is_ok()
}

/// Makes the game's tray icon if it has none (or its file is gone). Returns whether one was
/// stored. Sources, best first: the store's icon, the executable's icon (Windows), a square crop
/// of the cover.
fn fetch_icon(shared: &MetadataShared, id: Uuid) -> bool {
    let forget = || {
        shared
            .icons_seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    };
    let Ok(Some(game)) = shared.db.find_game(id) else {
        forget();
        return false;
    };
    let has_file = |path: &Option<String>| {
        path.as_deref()
            .is_some_and(|p| shared.images.resolve(p).is_some())
    };
    if has_file(&game.icon_path) {
        return false;
    }
    let Some(png) = build_icon(shared, &game) else {
        tracing::debug!(game = %id, "no icon found");
        if !has_file(&game.cover_path) {
            // The cover is the last resort and may still arrive; try again then.
            forget();
        }
        return false;
    };
    shared
        .images
        .store_image(ImageKind::Icon, id, "png", &png)
        .and_then(|rel| shared.db.set_icon_path(id, Some(&rel)))
        .inspect_err(|err| tracing::warn!(game = %id, %err, "could not store icon"))
        .is_ok()
}

/// The 32 px icon PNG for a game from the first source that yields a valid image.
fn build_icon(shared: &MetadataShared, game: &Game) -> Option<Vec<u8>> {
    let from_provider = || {
        let source_id = game.source_game_id.as_deref()?;
        let provider = shared.providers.get(&game.source)?;
        provider
            .icon_candidates(source_id)
            .iter()
            .find_map(|candidate| {
                let loaded = match candidate {
                    ImageCandidate::File(path) => load_file(path),
                    ImageCandidate::Url(url) => download(&shared.http, url),
                };
                icon_png_from_bytes(&loaded?.1, SquareFrom::Icon)
            })
    };
    let from_executable = || {
        let action = game.play_action.as_ref()?;
        if action.kind != ActionKind::File {
            return None;
        }
        let path = std::path::Path::new(&action.target);
        if !path.is_file() {
            return None;
        }
        let icon = crate::platform::executable_icon(path, EXECUTABLE_ICON_SIZE)?;
        icon_png_from_rgba(&icon, SquareFrom::Icon)
    };
    let from_cover = || {
        let path = shared.images.resolve(game.cover_path.as_deref()?)?;
        if std::fs::metadata(&path).ok()?.len() > MAX_IMAGE_BYTES {
            return None;
        }
        icon_png_from_bytes(&std::fs::read(path).ok()?, SquareFrom::CoverTop)
    };
    from_provider().or_else(from_executable).or_else(from_cover)
}

fn load_file(path: &std::path::Path) -> Option<(&'static str, Vec<u8>)> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    image_extension(&bytes).map(|ext| (ext, bytes))
}

pub(super) fn download(http: &ureq::Agent, url: &str) -> Option<(&'static str, Vec<u8>)> {
    let response = match http.get(url).call() {
        Ok(r) => r,
        Err(err) => {
            tracing::debug!(%url, %err, "cover download failed");
            return None;
        }
    };
    let mut bytes = Vec::new();
    let read = response
        .into_body()
        .into_reader()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > MAX_IMAGE_BYTES {
        return None;
    }
    image_extension(&bytes).map(|ext| (ext, bytes))
}

/// Detects the image type from magic bytes; rejects anything that isn't an image (e.g. an HTML
/// error page served with status 200).
pub fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("webp"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::db::NewGameRecord;
    use crate::model::{GameMetadata, ImportedGame};
    use crate::paths::DataPaths;
    use crate::providers::ProviderError;

    struct LocalCovers(std::path::PathBuf);
    impl LibraryProvider for LocalCovers {
        fn id(&self) -> &str {
            "local"
        }
        fn name(&self) -> &str {
            "Local"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(vec![])
        }
        fn cover_candidates(&self, id: &str) -> Vec<ImageCandidate> {
            vec![
                ImageCandidate::File(self.0.join("missing.jpg")),
                ImageCandidate::File(self.0.join(format!("{id}.bin"))),
            ]
        }
    }

    #[test]
    fn detects_image_types() {
        assert_eq!(image_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_extension(b"\x89PNG\r\n"), Some("png"));
        assert_eq!(image_extension(b"RIFF\0\0\0\0WEBPVP8"), Some("webp"));
        assert_eq!(image_extension(b"<html>"), None);
        assert_eq!(image_extension(b""), None);
    }

    #[test]
    fn fetches_first_valid_local_candidate_once() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let covers = tmp.path().join("covers");
        std::fs::create_dir_all(&covers).unwrap();
        std::fs::write(covers.join("1.bin"), [0xFF, 0xD8, 0xFF, 0xE0, 1, 2]).unwrap();
        std::fs::write(covers.join("2.bin"), b"not an image").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let mk = |id: &str| {
            db.insert_game(
                &NewGameRecord {
                    name: id.into(),
                    source: "local".into(),
                    source_game_id: Some(id.into()),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap()
        };
        let (g1, g2) = (mk("1"), mk("2"));
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(LocalCovers(covers)));
        let fetcher = CoverFetcher::new(
            db.clone(),
            ImageCache::new(paths),
            providers,
            EventBus::new(),
            SteamGridDb::new(None),
            2,
        );
        let job = |g: &crate::model::Game| CoverJob {
            game_id: g.id,
            source: "local".into(),
            source_game_id: g.source_game_id.clone(),
            force: false,
        };
        assert_eq!(fetcher.enqueue([job(&g1), job(&g2)]), 2);
        assert_eq!(
            fetcher.enqueue([job(&g1)]),
            0,
            "not retried in the same run"
        );

        let start = Instant::now();
        while db.get_game(g1.id).unwrap().cover_path.is_none() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            db.get_game(g1.id).unwrap().cover_path.unwrap(),
            format!("images/covers/{}.jpg", g1.id)
        );
        std::thread::sleep(Duration::from_millis(50));
        assert!(db.get_game(g2.id).unwrap().cover_path.is_none());
    }

    struct MetaStore(std::path::PathBuf);
    impl LibraryProvider for MetaStore {
        fn id(&self) -> &str {
            "meta"
        }
        fn name(&self) -> &str {
            "Meta"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(vec![])
        }
        fn metadata(&self, id: &str) -> Result<Option<GameMetadata>, ProviderError> {
            match id {
                "1" => Ok(Some(GameMetadata {
                    description: Some("A game.".into()),
                    genres: vec!["Puzzle".into()],
                    ..Default::default()
                })),
                "2" => Ok(None),
                _ => Err(ProviderError::Network("offline".into())),
            }
        }
        fn background_candidates(&self, _id: &str) -> Vec<ImageCandidate> {
            vec![ImageCandidate::File(self.0.clone())]
        }
    }

    #[test]
    fn fetches_metadata_and_background() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let bg = tmp.path().join("bg.png");
        std::fs::write(&bg, b"\x89PNG\r\n\x1a\n").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let mk = |id: &str| {
            db.insert_game(
                &NewGameRecord {
                    name: id.into(),
                    source: "meta".into(),
                    source_game_id: Some(id.into()),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap()
        };
        let games = [mk("1"), mk("2"), mk("3")];
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(MetaStore(bg)));
        let fetcher = MetadataFetcher::new(
            db.clone(),
            ImageCache::new(paths),
            providers,
            EventBus::new(),
            SteamGridDb::new(None),
            Duration::ZERO,
        );
        let jobs = games.iter().map(|g| MetadataJob {
            game_id: g.id,
            source: "meta".into(),
            source_game_id: g.source_game_id.clone().unwrap(),
        });
        assert_eq!(fetcher.enqueue(jobs), 3);

        let start = Instant::now();
        while db.get_game(games[2].id).unwrap().background_path.is_none() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
        let g1 = db.get_game(games[0].id).unwrap();
        assert_eq!(g1.description.as_deref(), Some("A game."));
        assert_eq!(g1.genres, vec!["Puzzle".to_string()]);
        assert!(g1.background_path.is_some());
        let g2 = db.get_game(games[1].id).unwrap();
        assert!(
            g2.metadata_updated_at.is_some(),
            "no data still marks checked"
        );
        let g3 = db.get_game(games[2].id).unwrap();
        assert!(g3.metadata_updated_at.is_none(), "errors are retried later");
    }

    struct IconStore(std::path::PathBuf);
    impl LibraryProvider for IconStore {
        fn id(&self) -> &str {
            "icons"
        }
        fn name(&self) -> &str {
            "Icons"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(vec![])
        }
        fn icon_candidates(&self, id: &str) -> Vec<ImageCandidate> {
            vec![
                ImageCandidate::File(self.0.join("missing.png")),
                // Wide images are not icons and are skipped.
                ImageCandidate::File(self.0.join("wide.png")),
                ImageCandidate::File(self.0.join(format!("{id}.png"))),
            ]
        }
    }

    fn png_bytes(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(w, h, image::Rgb(rgb))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn icon_jobs_use_the_store_icon_then_the_cover_and_retry_without_a_cover() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let src = tmp.path().join("icons");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("1.png"), png_bytes(64, 64, [200, 30, 30])).unwrap();
        std::fs::write(src.join("wide.png"), png_bytes(200, 50, [0, 0, 0])).unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let images = ImageCache::new(paths);
        let mk = |id: &str, source: &str, source_id: Option<&str>| {
            db.insert_game(
                &NewGameRecord {
                    name: id.into(),
                    source: source.into(),
                    source_game_id: source_id.map(str::to_string),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap()
        };
        let from_store = mk("store", "icons", Some("1"));
        let from_cover = mk("cover", "manual", None);
        let nothing = mk("nothing", "manual", None);
        let rel = images
            .store_image(
                ImageKind::Cover,
                from_cover.id,
                "png",
                &png_bytes(40, 60, [10, 200, 10]),
            )
            .unwrap();
        db.set_cover_path(from_cover.id, Some(&rel)).unwrap();

        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(IconStore(src)));
        let events = EventBus::new();
        let (tx, rx) = mpsc::channel();
        events.subscribe(move |event| {
            let _ = tx.send(event.clone());
        });
        let fetcher = MetadataFetcher::new(
            db.clone(),
            images.clone(),
            providers,
            events,
            SteamGridDb::new(None),
            Duration::ZERO,
        );
        let ids = [from_store.id, from_cover.id, nothing.id];
        assert_eq!(fetcher.enqueue_icons(ids), 3);
        assert_eq!(fetcher.enqueue_icons([from_store.id]), 0, "queued once");

        let start = Instant::now();
        loop {
            let done = |id| db.get_game(id).unwrap().icon_path.is_some();
            let forgotten = !fetcher
                .shared
                .icons_seen
                .lock()
                .unwrap()
                .contains(&nothing.id);
            if done(from_store.id) && done(from_cover.id) && forgotten {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "icons were not made"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let pixel = |id: Uuid| {
            let rel = db.get_game(id).unwrap().icon_path.unwrap();
            assert_eq!(rel, format!("images/icons/{id}.png"));
            let img = image::open(images.resolve(&rel).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(img.dimensions(), (32, 32));
            img.get_pixel(16, 16).0
        };
        assert_eq!(pixel(from_store.id), [200, 30, 30, 255]);
        assert_eq!(pixel(from_cover.id), [10, 200, 10, 255]);
        assert!(db.get_game(nothing.id).unwrap().icon_path.is_none());
        // Nothing to make an icon of, and no cover yet: asking again later is allowed.
        assert_eq!(fetcher.enqueue_icons([nothing.id]), 1);
        assert_eq!(
            fetcher.enqueue_icons([from_store.id]),
            0,
            "done games stay done"
        );
        let changed = std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|e| matches!(e, CoreEvent::GamesChanged { .. }))
            .count();
        assert!(changed >= 2, "icons announce themselves");
    }

    #[test]
    fn a_deleted_icon_file_is_made_again() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let db = Arc::new(Database::open_in_memory().unwrap());
        let images = ImageCache::new(paths);
        let game = db
            .insert_game(
                &NewGameRecord {
                    name: "g".into(),
                    source: "manual".into(),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap();
        let cover = images
            .store_image(
                ImageKind::Cover,
                game.id,
                "png",
                &png_bytes(60, 90, [1, 2, 3]),
            )
            .unwrap();
        db.set_cover_path(game.id, Some(&cover)).unwrap();
        let shared = MetadataShared {
            db: db.clone(),
            images: images.clone(),
            providers: ProviderRegistry::new(),
            events: EventBus::new(),
            artwork: SteamGridDb::new(None),
            http: http_agent(),
            interval: Duration::ZERO,
            queued: Mutex::new(HashSet::new()),
            icons_seen: Mutex::new(HashSet::new()),
        };
        assert!(fetch_icon(&shared, game.id));
        assert!(
            !fetch_icon(&shared, game.id),
            "present icons are left alone"
        );
        let rel = db.get_game(game.id).unwrap().icon_path.unwrap();
        std::fs::remove_file(images.resolve(&rel).unwrap()).unwrap();
        assert!(fetch_icon(&shared, game.id));
        assert!(images.resolve(&rel).is_some());
    }
}
