//! The public PC Game Pass catalog, used to leave Game Pass titles out of the owned list.
//!
//! `GET https://catalog.gamepass.com/sigls/v2?id=<list>&language=en-us&market=US` answers with a
//! JSON array: a header object (`siglId`, `title`, ...) followed by `{"id": "<Store id>"}` entries.
//! The list `fdd9e2a7-0fee-49f6-ad69-4354098401ff` is "All PC Games" (about 500 titles). No
//! account is needed. The Store ids are the catalog's `ProductId`s (see `catalog`).

use std::collections::HashSet;

use serde_json::Value;

use super::super::ProviderError;
use crate::auth::{Http, HttpRequest};

const ALL_PC_GAMES_URL: &str = "https://catalog.gamepass.com/sigls/v2?id=fdd9e2a7-0fee-49f6-ad69-4354098401ff&language=en-us&market=US";

/// The Store ids in a sigls answer, upper-cased.
pub fn parse_list(body: &str) -> Result<HashSet<String>, ProviderError> {
    let entries: Vec<Value> = serde_json::from_str(body).map_err(|_| {
        ProviderError::Parse("The Game Pass catalog returned an unexpected response.".into())
    })?;
    Ok(entries
        .iter()
        .filter_map(|e| e.get("id")?.as_str())
        .map(str::to_ascii_uppercase)
        .collect())
}

/// The Store ids of every game currently in PC Game Pass.
pub fn fetch_pc_games(http: &dyn Http) -> Result<HashSet<String>, ProviderError> {
    let response = http
        .send(&HttpRequest::get(ALL_PC_GAMES_URL).header("Accept", "application/json"))
        .map_err(ProviderError::Network)?;
    if !response.ok() {
        return Err(ProviderError::Network(format!(
            "The Game Pass catalog returned HTTP {}",
            response.status
        )));
    }
    parse_list(&response.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_store_ids_and_skips_the_header() {
        let body = r#"[{"siglId":"fdd9e2a7","title":"All PC Games"},{"id":"9NPDN9R45JX4"},{"id":"9pp5g1f0c2b6"}]"#;
        let ids = parse_list(body).unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("9PP5G1F0C2B6"));
        assert!(parse_list("{}").is_err());
    }
}
