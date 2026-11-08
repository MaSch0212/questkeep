//! The statistics page: playtime history, library figures, drives and this PC.
//!
//! Playtime per day comes from two places: the sessions Questkeep records (split over local days
//! and hours) and the stores' playtime growth between syncs that Questkeep didn't record itself
//! (`db::playtime`, per day only).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Datelike, Days, Local, NaiveDate, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::db::{
    Database, SessionSpan, StatCopy, StatEntry, StatSession, StoreDay, playtime_by_day,
};
use crate::error::Result;
pub use questkeep_system::{DriveInfo, DriveKind, SystemInfo};

/// How many games the "most played" list shows.
const TOP_GAMES: usize = 8;
/// How many games the chart shows on their own when split by game (the rest is "other").
const CHART_GAMES: usize = 5;
/// How many genres are listed.
const TOP_GENRES: usize = 6;
/// Weeks in the activity calendar.
const HEATMAP_WEEKS: u64 = 26;
/// Installed games not played for this long are clean-up candidates.
const STALE_DAYS: i64 = 365;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Statistics {
    #[ts(type = "number")]
    pub total_seconds: u64,
    pub games: u32,
    pub installed: u32,
    pub never_played: u32,
    #[ts(type = "number")]
    pub installed_bytes: u64,
    #[ts(type = "number")]
    pub this_week_seconds: u64,
    #[ts(type = "number")]
    pub last_week_seconds: u64,
    /// The first day with playtime history (`YYYY-MM-DD`); none before the first record.
    pub history_since: Option<String>,
    /// Every day of the last [`HEATMAP_WEEKS`] weeks, starting on a Monday, oldest first.
    pub calendar: Vec<DaySeconds>,
    pub current_streak: u32,
    pub longest_streak: u32,
    /// Recorded playtime per weekday (Monday first) and hour of day: 7 × 24 seconds.
    pub hours: Vec<Vec<u32>>,
    pub top_games: Vec<GameSeconds>,
    /// Playtime per store (store id), most first.
    pub stores: Vec<KeySeconds>,
    /// Playtime per genre, most first.
    pub genres: Vec<KeySeconds>,
    /// Games per completion status id (`None`: no status), most first.
    pub completion: Vec<StatusCount>,
    /// Games per release decade (1990, 2000, ...), oldest first.
    pub decades: Vec<DecadeCount>,
    pub drives: Vec<DriveUsage>,
    pub cleanup: Cleanup,
    pub facts: Facts,
    pub system: SystemInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DaySeconds {
    /// `YYYY-MM-DD`
    pub day: String,
    #[ts(type = "number")]
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameSeconds {
    pub id: Uuid,
    pub name: String,
    #[ts(type = "number")]
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeySeconds {
    pub key: String,
    #[ts(type = "number")]
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StatusCount {
    pub status: Option<String>,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DecadeCount {
    pub decade: i32,
    pub count: u32,
}

/// A drive with the space its installed games take.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DriveUsage {
    pub drive: DriveInfo,
    #[ts(type = "number")]
    pub games_bytes: u64,
    pub games: u32,
}

/// Installed games not played for a year (those with a known size).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Cleanup {
    #[ts(type = "number")]
    pub bytes: u64,
    pub count: u32,
    /// The largest ones, largest first.
    pub largest: Vec<GameBytes>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameBytes {
    pub id: Uuid,
    pub name: String,
    #[ts(type = "number")]
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Facts {
    pub longest_session: Option<GameSeconds>,
    #[ts(type = "number | null")]
    pub average_session_seconds: Option<u64>,
    /// Games never played, and the main-story time to beat of those that know it.
    pub backlog_games: u32,
    #[ts(type = "number")]
    pub backlog_seconds: u64,
    /// The most played game's share of all playtime (0–1).
    pub top_share: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChartRange {
    /// The last 7 days.
    Week,
    /// The last 30 days.
    Month,
    /// The last 12 months, per month.
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChartSplit {
    Total,
    Store,
    Game,
}

/// Playtime per day (or month) in one or more series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlaytimeChart {
    /// `YYYY-MM-DD` per day, `YYYY-MM` per month, oldest first.
    pub labels: Vec<String>,
    pub series: Vec<ChartSeries>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChartSeries {
    /// `total`, a store id, a game id, or `other`.
    pub key: String,
    /// The game's name when split by game.
    pub name: Option<String>,
    #[ts(type = "number[]")]
    pub values: Vec<u64>,
}

/// Seconds played by one entry (in one store) on one local day.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Played {
    entry: Uuid,
    source: String,
    day: NaiveDate,
    seconds: u64,
}

/// Splits sessions over the local days they ran on and adds the stores' daily growth; only days
/// from `from` to `today`.
fn played_days<Tz: TimeZone>(
    sessions: &[StatSession],
    store_days: &[StoreDay],
    from: NaiveDate,
    today: NaiveDate,
    tz: &Tz,
) -> Vec<Played> {
    let mut out = Vec::new();
    for session in sessions {
        let first = session.span.start.with_timezone(tz).date_naive();
        let last = session.span.end.with_timezone(tz).date_naive().min(today);
        let days = u32::try_from((last - first).num_days() + 1)
            .unwrap_or(1)
            .max(1);
        for day in playtime_by_day(std::slice::from_ref(&session.span), last, tz, days) {
            if day.seconds > 0 && day.date >= from && day.date <= today {
                out.push(Played {
                    entry: session.entry,
                    source: session.source.clone(),
                    day: day.date,
                    seconds: day.seconds,
                });
            }
        }
    }
    out.extend(
        store_days
            .iter()
            .filter(|d| d.day >= from && d.day <= today)
            .map(|d| Played {
                entry: d.entry,
                source: d.source.clone(),
                day: d.day,
                seconds: d.seconds,
            }),
    );
    out
}

/// Recorded seconds per weekday (Monday first) and local hour.
fn hour_grid<Tz: TimeZone>(sessions: &[SessionSpan], tz: &Tz) -> Vec<Vec<u32>> {
    let mut grid = vec![vec![0f64; 24]; 7];
    for span in sessions {
        let wall = (span.end - span.start).num_seconds();
        if wall <= 0 {
            let local = span.start.with_timezone(tz);
            let (d, h) = (local.weekday().num_days_from_monday(), local.hour());
            if let Some(slot) = grid.get_mut(d as usize).and_then(|r| r.get_mut(h as usize)) {
                *slot += span.credited as f64;
            }
            continue;
        }
        let rate = span.credited as f64 / wall as f64;
        let mut at = span.start;
        while at < span.end {
            let local = at.with_timezone(tz);
            let next_hour = at
                + chrono::Duration::seconds(i64::from(3600 - local.minute() * 60 - local.second()));
            let until = next_hour.min(span.end);
            let (d, h) = (local.weekday().num_days_from_monday(), local.hour());
            if let Some(slot) = grid.get_mut(d as usize).and_then(|r| r.get_mut(h as usize)) {
                *slot += (until - at).num_seconds() as f64 * rate;
            }
            at = until;
        }
    }
    grid.into_iter()
        .map(|row| row.into_iter().map(|s| s.round() as u32).collect())
        .collect()
}

/// The longest run of consecutive played days, and the run ending today (or yesterday, so a
/// streak isn't broken before today's session).
fn streaks(days: &BTreeSet<NaiveDate>, today: NaiveDate) -> (u32, u32) {
    let mut longest = 0;
    let mut run = 0;
    let mut previous: Option<NaiveDate> = None;
    for &day in days {
        run = if previous.and_then(|p| p.succ_opt()) == Some(day) {
            run + 1
        } else {
            1
        };
        longest = longest.max(run);
        previous = Some(day);
    }
    let mut current = 0;
    let mut day = if days.contains(&today) {
        Some(today)
    } else {
        today.pred_opt()
    };
    while let Some(d) = day.filter(|d| days.contains(d)) {
        current += 1;
        day = d.pred_opt();
    }
    (current, longest)
}

/// The mount a folder is on: the longest matching mount point (case-insensitive on Windows).
fn drive_of<'a>(drives: &'a [DriveInfo], dir: &str) -> Option<&'a DriveInfo> {
    let norm = |s: &str| {
        let s = s.replace('\\', "/");
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    let dir = norm(dir);
    drives
        .iter()
        .filter(|d| dir.starts_with(&norm(&d.mount)))
        .max_by_key(|d| d.mount.len())
}

fn top<K: Ord + Clone>(map: HashMap<K, u64>, limit: usize) -> Vec<(K, u64)> {
    let mut items: Vec<(K, u64)> = map.into_iter().filter(|(_, s)| *s > 0).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(limit);
    items
}

/// Everything but the chart, from the database and `system` (read by the caller, it's slow-ish).
pub fn statistics(
    db: &Database,
    fake_source: &str,
    system: SystemInfo,
    now: DateTime<Utc>,
) -> Result<Statistics> {
    compute(db, fake_source, system, now, &Local)
}

fn compute<Tz: TimeZone>(
    db: &Database,
    fake_source: &str,
    system: SystemInfo,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<Statistics> {
    let today = now.with_timezone(tz).date_naive();
    let entries = db.stat_entries(fake_source)?;
    let copies = db.stat_copies(fake_source)?;
    let names: HashMap<Uuid, &str> = entries.iter().map(|e| (e.id, e.name.as_str())).collect();
    let known: BTreeSet<Uuid> = entries.iter().map(|e| e.id).collect();

    // Playtime history (all of it: it only exists since this version).
    let history_since = db.history_start()?;
    let first = history_since
        .as_deref()
        .and_then(|d| d.get(..10)?.parse::<NaiveDate>().ok())
        .map_or(today, |d| d.min(today))
        .checked_sub_days(Days::new(1))
        .unwrap_or(today);
    let sessions: Vec<StatSession> = db
        .stat_sessions_since(DateTime::<Utc>::MIN_UTC)?
        .into_iter()
        .filter(|s| known.contains(&s.entry))
        .collect();
    let store_days: Vec<StoreDay> = db
        .store_days_since(first)?
        .into_iter()
        .filter(|d| known.contains(&d.entry))
        .collect();
    let played = played_days(&sessions, &store_days, first, today, tz);
    let mut per_day: BTreeMap<NaiveDate, u64> = BTreeMap::new();
    for p in &played {
        *per_day.entry(p.day).or_default() += p.seconds;
    }
    let monday = today
        .checked_sub_days(Days::new(u64::from(today.weekday().num_days_from_monday())))
        .unwrap_or(today);
    let last_monday = monday.checked_sub_days(Days::new(7)).unwrap_or(monday);
    let sum_days =
        |from: NaiveDate, to: NaiveDate| -> u64 { per_day.range(from..to).map(|(_, s)| s).sum() };
    let this_week_seconds = sum_days(monday, today.succ_opt().unwrap_or(today));
    let last_week_seconds = sum_days(last_monday, monday);
    let calendar_start = monday
        .checked_sub_days(Days::new((HEATMAP_WEEKS - 1) * 7))
        .unwrap_or(monday);
    let calendar = calendar_start
        .iter_days()
        .take_while(|d| *d <= today)
        .map(|d| DaySeconds {
            day: d.to_string(),
            seconds: per_day.get(&d).copied().unwrap_or_default(),
        })
        .collect();
    let played_set: BTreeSet<NaiveDate> = per_day
        .iter()
        .filter(|(_, s)| **s >= 60)
        .map(|(d, _)| *d)
        .collect();
    let (current_streak, longest_streak) = streaks(&played_set, today);
    let spans: Vec<SessionSpan> = sessions.iter().map(|s| s.span).collect();
    let hours = hour_grid(&spans, tz);

    // Library figures.
    let total_seconds: u64 = entries.iter().map(|e| e.playtime_seconds).sum();
    let never: Vec<&StatEntry> = entries
        .iter()
        .filter(|e| e.playtime_seconds == 0 && e.play_count == 0)
        .collect();
    let mut by_entry: Vec<&StatEntry> = entries.iter().filter(|e| e.playtime_seconds > 0).collect();
    by_entry.sort_by(|a, b| {
        b.playtime_seconds
            .cmp(&a.playtime_seconds)
            .then(a.name.cmp(&b.name))
    });
    let top_games = by_entry
        .iter()
        .take(TOP_GAMES)
        .map(|e| GameSeconds {
            id: e.id,
            name: e.name.clone(),
            seconds: e.playtime_seconds,
        })
        .collect();
    let mut stores: HashMap<String, u64> = HashMap::new();
    for copy in &copies {
        *stores.entry(copy.source.clone()).or_default() += copy.playtime_seconds;
    }
    let mut genres: HashMap<String, u64> = HashMap::new();
    for entry in &entries {
        for genre in &entry.genres {
            *genres.entry(genre.clone()).or_default() += entry.playtime_seconds;
        }
    }
    // Placeholder dates (9999) are no release year.
    let latest_year = today.year() + 5;
    let mut completion: HashMap<Option<String>, u32> = HashMap::new();
    let mut decades: BTreeMap<i32, u32> = BTreeMap::new();
    for entry in &entries {
        *completion
            .entry(entry.completion_status.clone())
            .or_default() += 1;
        if let Some(year) = entry
            .release_year
            .filter(|y| (1950..=latest_year).contains(y))
        {
            *decades.entry(year - year.rem_euclid(10)).or_default() += 1;
        }
    }
    let mut completion: Vec<StatusCount> = completion
        .into_iter()
        .map(|(status, count)| StatusCount { status, count })
        .collect();
    completion.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.status.cmp(&b.status)));

    // Drives and clean-up.
    let installed_copies: Vec<&StatCopy> =
        copies.iter().filter(|c| c.install_dir.is_some()).collect();
    let installed_bytes = installed_copies.iter().filter_map(|c| c.install_size).sum();
    let mut drives: Vec<DriveUsage> = system
        .drives
        .iter()
        .map(|d| DriveUsage {
            drive: d.clone(),
            games_bytes: 0,
            games: 0,
        })
        .collect();
    for copy in &installed_copies {
        let Some(dir) = copy.install_dir.as_deref() else {
            continue;
        };
        if let Some(mount) = drive_of(&system.drives, dir).map(|d| d.mount.clone())
            && let Some(usage) = drives.iter_mut().find(|u| u.drive.mount == mount)
        {
            usage.games += 1;
            usage.games_bytes += copy.install_size.unwrap_or_default();
        }
    }
    let stale_before = now - chrono::Duration::days(STALE_DAYS);
    let mut stale: Vec<GameBytes> = entries
        .iter()
        .filter(|e| e.installed && e.last_played.is_some_and(|at| at < stale_before))
        .map(|e| GameBytes {
            id: e.id,
            name: e.name.clone(),
            bytes: installed_copies
                .iter()
                .filter(|c| c.entry == e.id)
                .filter_map(|c| c.install_size)
                .sum(),
        })
        .filter(|g| g.bytes > 0)
        .collect();
    stale.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    let cleanup = Cleanup {
        bytes: stale.iter().map(|g| g.bytes).sum(),
        count: u32::try_from(stale.len()).unwrap_or(u32::MAX),
        largest: stale.into_iter().take(3).collect(),
    };

    // Fun facts.
    let longest_session = db.longest_session()?.and_then(|(id, seconds)| {
        Some(GameSeconds {
            id,
            name: (*names.get(&id)?).to_string(),
            seconds,
        })
    });
    let (session_count, session_total) = db.session_totals()?;
    let facts = Facts {
        longest_session,
        average_session_seconds: (session_count > 0).then(|| session_total / session_count),
        backlog_games: u32::try_from(never.len()).unwrap_or(u32::MAX),
        backlog_seconds: never.iter().filter_map(|e| e.ttb_main).sum(),
        top_share: by_entry
            .first()
            .filter(|_| total_seconds > 0)
            .map(|e| e.playtime_seconds as f64 / total_seconds as f64),
    };

    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Ok(Statistics {
        total_seconds,
        games: count(entries.len()),
        installed: count(entries.iter().filter(|e| e.installed).count()),
        never_played: count(never.len()),
        installed_bytes,
        this_week_seconds,
        last_week_seconds,
        history_since,
        calendar,
        current_streak,
        longest_streak,
        hours,
        top_games,
        stores: top(stores, usize::MAX)
            .into_iter()
            .map(|(key, seconds)| KeySeconds { key, seconds })
            .collect(),
        genres: top(genres, TOP_GENRES)
            .into_iter()
            .map(|(key, seconds)| KeySeconds { key, seconds })
            .collect(),
        completion,
        decades: decades
            .into_iter()
            .map(|(decade, count)| DecadeCount { decade, count })
            .collect(),
        drives,
        cleanup,
        facts,
        system,
    })
}

/// Playtime per day (7 or 30 days) or month (12 months) up to today, as one total or per store or
/// game.
pub fn playtime_chart(
    db: &Database,
    fake_source: &str,
    range: ChartRange,
    split: ChartSplit,
    now: DateTime<Utc>,
) -> Result<PlaytimeChart> {
    chart(db, fake_source, range, split, now, &Local)
}

fn chart<Tz: TimeZone>(
    db: &Database,
    fake_source: &str,
    range: ChartRange,
    split: ChartSplit,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<PlaytimeChart> {
    let today = now.with_timezone(tz).date_naive();
    let (from, labels): (NaiveDate, Vec<String>) = match range {
        ChartRange::Week | ChartRange::Month => {
            let days = if range == ChartRange::Week { 7 } else { 30 };
            let from = today.checked_sub_days(Days::new(days - 1)).unwrap_or(today);
            (
                from,
                from.iter_days()
                    .take(days as usize)
                    .map(|d| d.to_string())
                    .collect(),
            )
        }
        ChartRange::Year => {
            let this_month = today.with_day(1).unwrap_or(today);
            let from = this_month
                .checked_sub_months(chrono::Months::new(11))
                .unwrap_or(this_month);
            let labels = (0..12)
                .filter_map(|i| from.checked_add_months(chrono::Months::new(i)))
                .map(|d| d.format("%Y-%m").to_string())
                .collect();
            (from, labels)
        }
    };
    let bucket = |day: NaiveDate| -> Option<usize> {
        match range {
            ChartRange::Week | ChartRange::Month => usize::try_from((day - from).num_days()).ok(),
            ChartRange::Year => {
                let months =
                    (day.year() - from.year()) * 12 + day.month() as i32 - from.month() as i32;
                usize::try_from(months).ok()
            }
        }
    };
    let entries = db.stat_entries(fake_source)?;
    let names: HashMap<Uuid, String> = entries.iter().map(|e| (e.id, e.name.clone())).collect();
    let since = tz
        .from_local_datetime(&from.and_time(chrono::NaiveTime::MIN))
        .earliest()
        .map_or(DateTime::<Utc>::MIN_UTC, |t| t.with_timezone(&Utc))
        - chrono::Duration::days(1);
    let sessions: Vec<StatSession> = db
        .stat_sessions_since(since)?
        .into_iter()
        .filter(|s| names.contains_key(&s.entry))
        .collect();
    let store_days: Vec<StoreDay> = db
        .store_days_since(from)?
        .into_iter()
        .filter(|d| names.contains_key(&d.entry))
        .collect();
    let played = played_days(&sessions, &store_days, from, today, tz);
    let key_of = |p: &Played| -> String {
        match split {
            ChartSplit::Total => "total".to_string(),
            ChartSplit::Store => p.source.clone(),
            ChartSplit::Game => p.entry.to_string(),
        }
    };
    let mut totals: HashMap<String, u64> = HashMap::new();
    for p in &played {
        *totals.entry(key_of(p)).or_default() += p.seconds;
    }
    let mut keys: Vec<String> = top(totals, usize::MAX)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    if split == ChartSplit::Total && keys.is_empty() {
        keys.push("total".to_string());
    }
    let shown: Vec<String> = if split == ChartSplit::Game && keys.len() > CHART_GAMES {
        keys.iter()
            .take(CHART_GAMES)
            .cloned()
            .chain(["other".to_string()])
            .collect()
    } else {
        keys.clone()
    };
    let mut values: HashMap<String, Vec<u64>> = shown
        .iter()
        .map(|k| (k.clone(), vec![0; labels.len()]))
        .collect();
    for p in &played {
        let mut key = key_of(p);
        if !values.contains_key(&key) {
            key = "other".to_string();
        }
        if let (Some(i), Some(series)) = (bucket(p.day), values.get_mut(&key))
            && let Some(slot) = series.get_mut(i)
        {
            *slot += p.seconds;
        }
    }
    let series = shown
        .into_iter()
        .map(|key| ChartSeries {
            name: (split == ChartSplit::Game)
                .then(|| {
                    Uuid::parse_str(&key)
                        .ok()
                        .and_then(|id| names.get(&id).cloned())
                })
                .flatten(),
            values: values.remove(&key).unwrap_or_default(),
            key,
        })
        .collect();
    Ok(PlaytimeChart { labels, series })
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;
    use crate::db::NewGameRecord;
    use crate::model::ImportedGame;

    fn at(d: u32, h: u32, m: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(2026, 11, d)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
            .and_utc()
    }

    fn span(start: DateTime<Utc>, minutes: i64) -> SessionSpan {
        SessionSpan {
            start,
            end: start + chrono::Duration::minutes(minutes),
            credited: u64::try_from(minutes * 60).unwrap(),
        }
    }

    fn system() -> SystemInfo {
        SystemInfo {
            os: "Test OS".into(),
            cpu: None,
            memory_bytes: 1,
            uptime_seconds: 1,
            gpus: vec![],
            displays: vec![],
            drives: vec![
                DriveInfo {
                    mount: "C:\\".into(),
                    label: None,
                    total_bytes: 1000,
                    free_bytes: 500,
                    kind: DriveKind::Ssd,
                    file_system: None,
                },
                DriveInfo {
                    mount: "C:\\Games\\".into(),
                    label: None,
                    total_bytes: 2000,
                    free_bytes: 1000,
                    kind: DriveKind::Hdd,
                    file_system: None,
                },
            ],
        }
    }

    #[test]
    fn sessions_are_split_over_hours_and_weekdays() {
        // Wednesday 2026-11-04, 22:30 to 00:30 (UTC).
        let grid = hour_grid(&[span(at(4, 22, 30), 120)], &Utc);
        assert_eq!(grid[2][22], 1800);
        assert_eq!(grid[2][23], 3600);
        assert_eq!(grid[3][0], 1800);
        assert_eq!(
            grid.iter().flatten().map(|s| u64::from(*s)).sum::<u64>(),
            7200
        );
    }

    #[test]
    fn streaks_count_consecutive_days() {
        let d = |n: u32| NaiveDate::from_ymd_opt(2026, 11, n).unwrap();
        let days: BTreeSet<NaiveDate> = [1, 2, 3, 5, 6].into_iter().map(d).collect();
        assert_eq!(streaks(&days, d(7)), (2, 3), "yesterday keeps it going");
        assert_eq!(streaks(&days, d(8)), (0, 3));
        assert_eq!(streaks(&BTreeSet::new(), d(8)), (0, 0));
    }

    #[test]
    fn folders_belong_to_the_deepest_mount() {
        let system = system();
        let mount = |dir: &str| drive_of(&system.drives, dir).map(|d| d.mount.as_str());
        assert_eq!(mount("C:\\Games\\Hades"), Some("C:\\Games\\"));
        assert_eq!(mount("C:\\Program Files\\Epic"), Some("C:\\"));
        assert_eq!(mount("Z:\\Other"), None);
    }

    #[test]
    fn statistics_and_chart_combine_sessions_and_store_growth() {
        let db = Database::open_in_memory().unwrap();
        let mut hades = ImportedGame::installed("1", "Hades");
        hades.install_dir = Some("C:\\Games\\Hades".into());
        hades.playtime_seconds = Some(36_000);
        let mut celeste = ImportedGame::installed("2", "Celeste");
        celeste.installed = false;
        celeste.playtime_seconds = Some(0);
        db.upsert_imported("steam", &[hades.clone(), celeste], at(1, 0, 0))
            .unwrap();
        let manual = db
            .insert_game(
                &NewGameRecord {
                    name: "Doom".into(),
                    source: "manual".into(),
                    ..Default::default()
                },
                at(1, 0, 0),
            )
            .unwrap();
        let day = |d: u32| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        // Store baseline, then two more hours on the 3rd.
        db.record_store_playtime("steam", &[("1".into(), 36_000)], at(2, 0, 0), day(2))
            .unwrap();
        db.record_store_playtime("steam", &[("1".into(), 43_200)], at(3, 20, 0), day(3))
            .unwrap();
        // A recorded hour of Doom on the 4th.
        let session = db.begin_session(manual.id, at(4, 18, 0)).unwrap();
        db.finish_session(session, at(4, 19, 0), 3_600).unwrap();

        let now = at(5, 12, 0);
        let stats = compute(&db, "fake", system(), now, &Utc).unwrap();
        assert_eq!(stats.games, 3);
        assert_eq!(stats.never_played, 1);
        assert_eq!(stats.this_week_seconds, 7_200 + 3_600, "Mon 2nd .. Thu 5th");
        assert_eq!(stats.longest_streak, 2);
        assert_eq!(stats.hours[2][18], 3_600, "Wednesday 18:00");
        assert_eq!(
            stats.top_games.first().map(|g| g.name.as_str()),
            Some("Hades")
        );
        assert_eq!(stats.drives[1].games, 1, "Hades is on C:\\Games");
        assert_eq!(
            stats.facts.longest_session.as_ref().map(|g| g.seconds),
            Some(3_600)
        );
        assert_eq!(
            stats.calendar.last().map(|d| d.day.as_str()),
            Some("2026-11-05")
        );

        let week = chart(&db, "fake", ChartRange::Week, ChartSplit::Store, now, &Utc).unwrap();
        assert_eq!(week.labels.first().map(String::as_str), Some("2026-10-30"));
        let steam = week.series.iter().find(|s| s.key == "steam").unwrap();
        assert_eq!(steam.values[4], 7_200, "the 3rd");
        let manual_series = week.series.iter().find(|s| s.key == "manual").unwrap();
        assert_eq!(manual_series.values[5], 3_600, "the 4th");

        let months = chart(
            &db,
            "fake",
            ChartRange::Year,
            ChartSplit::Game,
            now,
            &FixedOffset::east_opt(3600).unwrap(),
        )
        .unwrap();
        assert_eq!(months.labels.last().map(String::as_str), Some("2026-11"));
        assert_eq!(
            months.series.iter().map(|s| s.values[11]).sum::<u64>(),
            10_800
        );
        assert!(
            months
                .series
                .iter()
                .any(|s| s.name.as_deref() == Some("Doom"))
        );
    }
}
