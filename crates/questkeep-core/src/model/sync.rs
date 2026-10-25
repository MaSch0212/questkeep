use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// When stores last finished a sync, kept across restarts (RFC 3339 timestamps).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncTimes {
    /// The last "sync all" that finished.
    pub all: Option<String>,
    /// The last successful sync per provider id.
    pub providers: BTreeMap<String, String>,
}
