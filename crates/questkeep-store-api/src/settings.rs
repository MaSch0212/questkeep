//! A store's own settings, declared by the store and drawn by the app in the store's row of the
//! Stores settings. Values live in the settings file under `providers.<store id>.<key>` and reach
//! the store as its `config`; secret fields are kept in the system keyring instead.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoreSettings {
    /// Switches that take effect right away; the store is synced again after a change.
    pub options: Vec<StoreOption>,
    /// Credentials typed into a form and saved together, as an alternative to signing in.
    pub credentials: Option<StoreCredentials>,
}

/// A checkbox (`true` when ticked, unset otherwise).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoreOption {
    pub key: String,
    pub label: String,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoreCredentials {
    /// What the credentials are called in buttons and messages, e.g. `Steam key`.
    pub noun: String,
    /// The button that opens the form while nothing is saved.
    pub offer: String,
    /// The account badge while saved and not signed in, e.g. `API key`.
    pub badge: String,
    /// The line under the store name while saved and not signed in.
    pub detail: String,
    /// Shown above the fields.
    pub intro: String,
    /// Where to get the credentials, shown after the intro.
    pub link: Option<String>,
    pub fields: Vec<CredentialField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CredentialField {
    pub key: String,
    pub label: String,
    pub hint: Option<String>,
    /// Kept in the system keyring and typed into a masked input.
    pub secret: bool,
    /// The credentials count as saved when every required field has a value.
    pub required: bool,
    /// A number (numeric keyboard).
    pub numeric: bool,
}

impl StoreSettings {
    /// Keys of the secret fields.
    pub fn secret_keys(&self) -> impl Iterator<Item = &str> {
        self.credentials
            .iter()
            .flat_map(|c| &c.fields)
            .filter(|f| f.secret)
            .map(|f| f.key.as_str())
    }
}
