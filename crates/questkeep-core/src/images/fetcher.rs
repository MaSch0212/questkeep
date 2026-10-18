//! Background cover fetching with bounded concurrency.
//!
//! A fixed number of worker threads pull jobs from a channel, so at most `workers` downloads run
//! at once no matter how many games are queued. Each job tries the provider's cover candidates in
//! order (local files before URLs) and stores the first valid image.

use std::collections::HashSet;
use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use uuid::Uuid;

use super::{ImageCache, MAX_IMAGE_BYTES};
use crate::db::Database;
use crate::events::{CoreEvent, EventBus};
use crate::providers::{CoverCandidate, ProviderRegistry};

/// One game whose cover should be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverJob {
    pub game_id: Uuid,
    pub source: String,
    pub source_game_id: String,
}

struct Shared {
    db: Arc<Database>,
    images: ImageCache,
    providers: ProviderRegistry,
    events: EventBus,
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
        workers: usize,
    ) -> Self {
        let http: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(concat!("Questkeep/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        let shared = Arc::new(Shared {
            db,
            images,
            providers,
            events,
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
            if attempted.insert(job.game_id) && sender.send(job).is_ok() {
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
    let Some(provider) = shared.providers.get(&job.source) else {
        return;
    };
    for candidate in provider.cover_candidates(&job.source_game_id) {
        let loaded = match &candidate {
            CoverCandidate::File(path) => load_file(path),
            CoverCandidate::Url(url) => download(&shared.http, url),
        };
        let Some((ext, bytes)) = loaded else {
            continue;
        };
        let stored = shared
            .images
            .store_cover_bytes(job.game_id, ext, &bytes)
            .and_then(|rel| shared.db.set_cover_path(job.game_id, Some(&rel)));
        match stored {
            Ok(()) => {
                tracing::debug!(game = %job.game_id, ?candidate, "cover stored");
                shared.events.publish(CoreEvent::GamesChanged {
                    ids: vec![job.game_id],
                });
            }
            // The game may have been removed meanwhile; nothing else to do.
            Err(err) => tracing::warn!(game = %job.game_id, %err, "could not store cover"),
        }
        return;
    }
    tracing::debug!(game = %job.game_id, "no cover found");
}

fn load_file(path: &std::path::Path) -> Option<(&'static str, Vec<u8>)> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    image_extension(&bytes).map(|ext| (ext, bytes))
}

fn download(http: &ureq::Agent, url: &str) -> Option<(&'static str, Vec<u8>)> {
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
    use std::time::Instant;

    use chrono::Utc;

    use super::*;
    use crate::db::NewGameRecord;
    use crate::model::ImportedGame;
    use crate::paths::DataPaths;
    use crate::providers::{LibraryProvider, ProviderError};

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
        fn cover_candidates(&self, id: &str) -> Vec<CoverCandidate> {
            vec![
                CoverCandidate::File(self.0.join("missing.jpg")),
                CoverCandidate::File(self.0.join(format!("{id}.bin"))),
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
            2,
        );
        let job = |g: &crate::model::Game| CoverJob {
            game_id: g.id,
            source: "local".into(),
            source_game_id: g.source_game_id.clone().unwrap(),
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
}
