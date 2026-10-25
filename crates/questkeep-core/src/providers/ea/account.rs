//! EA account: owned games from EA's GraphQL service (the one the EA app uses), with a token
//! from the EA website session.
//!
//! The EA app's own sign-in needs a signature of the machine's hardware, so Questkeep signs in on
//! EA's website instead (in the login window, with a browser profile of its own; see
//! [`WebAccount`](crate::auth::WebAccount)). Before a scan a hidden window opens
//! `accounts.ea.com/connect/auth` for the `ORIGIN_JS_SDK` client with `prompt=none`, which answers
//! a signed-in session with a short-lived access token as JSON; the page's script reports it
//! through [`CAPTURE_PREFIX`] (`?token=...`, or `?signedout=1`). Lutris reads EA libraries the
//! same way. The token is only kept in memory.

use serde_json::{Value, json};

use crate::auth::{Http, HttpRequest, WebAccountSpec};
use crate::providers::ProviderError;

use super::EA_SOURCE;

/// The page the login window opens: EA's website login, which returns to `www.ea.com`.
pub const LOGIN_URL: &str = "https://accounts.ea.com/connect/auth?response_type=code&client_id=EADOTCOM-WEB-SERVER&display=junoWeb%2Flogin&locale=en_US&redirect_uri=https%3A%2F%2Fwww.ea.com%2Flogin_check";
/// Where EA's website login ends; the login window stops there, the session is then set.
pub const LOGIN_DONE_PREFIX: &str = "https://www.ea.com/login_check";
/// The page the hidden window opens: a token for the signed-in session, as JSON.
pub const SESSION_URL: &str = "https://accounts.ea.com/connect/auth?client_id=ORIGIN_JS_SDK&response_type=token&redirect_uri=nucleus%3Arest&prompt=none";
/// Navigations to this address carry what the token page reported; the window never loads it.
pub const CAPTURE_PREFIX: &str = "https://questkeep.invalid/ea-session";
const API_URL: &str = "https://service-aggregation-layer.juno.ea.com/graphql";
/// Offers per `legacyOffers` request.
const OFFER_BATCH: usize = 100;
/// Guards against a service that keeps returning a next page.
const MAX_PAGES: usize = 50;

pub const ACCOUNT_SPEC: WebAccountSpec = WebAccountSpec {
    provider: EA_SOURCE,
    name: "EA",
    login_url: LOGIN_URL,
    capture_prefix: CAPTURE_PREFIX,
};

const OWNED_QUERY: &str = r#"query ownedGames($limit: Int, $next: String) {
  me {
    ownedGameProducts(locale: "DEFAULT", entitlementEnabled: true, storefronts: [EA],
      type: [DIGITAL_FULL_GAME, PACKAGED_FULL_GAME], platforms: [PC],
      paging: {limit: $limit, next: $next}) {
      next
      items { originOfferId product { name baseItem { title gameType } } }
    }
  }
}"#;

const OFFERS_QUERY: &str = r#"query offers($offerIds: [String!]!) {
  legacyOffers(offerIds: $offerIds, locale: "DEFAULT") { id contentId }
}"#;

const NAME_QUERY: &str = "query { me { player { displayName } } }";

/// Accepts only plausible access tokens (EA's are opaque, base64-like strings or JWTs).
pub fn plausible_token(token: &str) -> bool {
    (16..=8192).contains(&token.len())
        && token.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'+' | b'/' | b'=')
        })
}

/// One owned game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedOffer {
    pub offer_id: String,
    /// The id installed games carry in their manifest (`contentID`).
    pub content_id: Option<String>,
    pub title: String,
}

fn graphql(
    http: &dyn Http,
    token: &str,
    query: &str,
    variables: Value,
) -> Result<Value, ProviderError> {
    let body = json!({ "query": query, "variables": variables }).to_string();
    let request = HttpRequest::post_json(API_URL, body)
        .bearer(token)
        .header("Accept", "application/json");
    let response = http.send(&request).map_err(ProviderError::Network)?;
    if response.status == 401 || response.status == 403 {
        return Err(ProviderError::NotConfigured(
            "EA did not accept the sign-in. Sign in to EA again in Settings.".into(),
        ));
    }
    if !response.ok() {
        return Err(ProviderError::Network(format!(
            "EA answered with HTTP {}",
            response.status
        )));
    }
    let value: Value = serde_json::from_str(&response.body)
        .map_err(|e| ProviderError::Parse(format!("EA games list: {e}")))?;
    if let Some(errors) = value
        .get("errors")
        .and_then(Value::as_array)
        .filter(|e| !e.is_empty())
    {
        let message = errors
            .iter()
            .filter_map(|e| e.get("message").and_then(Value::as_str))
            .next()
            .unwrap_or("unknown error");
        // Partial data is still usable; only fail without any.
        if value.get("data").is_none_or(Value::is_null) {
            return Err(ProviderError::Parse(format!("EA: {message}")));
        }
        tracing::debug!(message, "EA answered with errors");
    }
    Ok(value)
}

fn text(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The owned base games of the account behind `token`.
pub fn fetch_owned(http: &dyn Http, token: &str) -> Result<Vec<OwnedOffer>, ProviderError> {
    let mut offers: Vec<OwnedOffer> = Vec::new();
    let mut next: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let page = graphql(
            http,
            token,
            OWNED_QUERY,
            json!({ "limit": 100, "next": next }),
        )?;
        let owned = page
            .pointer("/data/me/ownedGameProducts")
            .ok_or_else(|| ProviderError::Parse("EA did not return the owned games.".into()))?;
        for item in owned
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(offer_id) = text(item, "/originOfferId") else {
                continue;
            };
            // Add-ons and other extras are not games of their own.
            if text(item, "/product/baseItem/gameType").is_some_and(|t| t != "BASE_GAME") {
                continue;
            }
            let Some(title) =
                text(item, "/product/baseItem/title").or_else(|| text(item, "/product/name"))
            else {
                continue;
            };
            if !offers.iter().any(|o| o.offer_id == offer_id) {
                offers.push(OwnedOffer {
                    offer_id,
                    content_id: None,
                    title,
                });
            }
        }
        next = text(owned, "/next");
        if next.is_none() {
            break;
        }
    }
    let ids: Vec<String> = offers.iter().map(|o| o.offer_id.clone()).collect();
    for batch in ids.chunks(OFFER_BATCH) {
        let result = graphql(http, token, OFFERS_QUERY, json!({ "offerIds": batch }))?;
        for offer in result
            .pointer("/data/legacyOffers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(id), Some(content_id)) = (text(offer, "/id"), text(offer, "/contentId"))
            else {
                continue;
            };
            if let Some(owned) = offers.iter_mut().find(|o| o.offer_id == id) {
                owned.content_id = Some(content_id);
            }
        }
    }
    Ok(offers)
}

/// The account's EA ID, for the settings page.
pub fn fetch_display_name(http: &dyn Http, token: &str) -> Option<String> {
    match graphql(http, token, NAME_QUERY, json!({})) {
        Ok(value) => text(&value, "/data/me/player/displayName"),
        Err(err) => {
            tracing::debug!(%err, "could not read the EA ID");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::testing::{FakeHttp, response};

    fn owned_page(next: Option<&str>, items: Value) -> String {
        json!({ "data": { "me": { "ownedGameProducts": { "next": next, "items": items } } } })
            .to_string()
    }

    #[test]
    fn reads_owned_games_across_pages_with_content_ids() {
        let http = FakeHttp::new(|req| {
            assert_eq!(req.url, API_URL);
            assert!(
                req.headers
                    .iter()
                    .any(|(k, v)| k == "Authorization" && v == "Bearer tok-123456789012")
            );
            let body: Value = serde_json::from_str(&req.body).unwrap();
            let query = body["query"].as_str().unwrap();
            if query.contains("ownedGameProducts") {
                if body["variables"]["next"].is_null() {
                    response(
                        200,
                        &owned_page(
                            Some("p2"),
                            json!([
                                { "originOfferId": "Origin.OFR.50.0000500",
                                  "product": { "name": "x", "baseItem": { "title": "Dragon Age™: Inquisition", "gameType": "BASE_GAME" } } },
                                { "originOfferId": "Origin.OFR.50.0000999",
                                  "product": { "name": "DLC", "baseItem": { "title": "Some DLC", "gameType": "EXTRA_CONTENT" } } }
                            ]),
                        ),
                    )
                } else {
                    response(
                        200,
                        &owned_page(
                            None,
                            json!([
                                { "originOfferId": "Origin.OFR.50.0000777", "product": { "name": "Mass Effect", "baseItem": null } },
                                { "originOfferId": "Origin.OFR.50.0000500", "product": { "baseItem": { "title": "dupe", "gameType": "BASE_GAME" } } },
                                { "product": null }
                            ]),
                        ),
                    )
                }
            } else {
                assert_eq!(body["variables"]["offerIds"].as_array().unwrap().len(), 2);
                response(
                    200,
                    &json!({ "data": { "legacyOffers": [
                    { "id": "Origin.OFR.50.0000500", "contentId": "1000659" },
                    { "id": "Origin.OFR.50.0000777", "contentId": null }
                ] } })
                    .to_string(),
                )
            }
        });
        let offers = fetch_owned(&http, "tok-123456789012").unwrap();
        assert_eq!(
            offers,
            [
                OwnedOffer {
                    offer_id: "Origin.OFR.50.0000500".into(),
                    content_id: Some("1000659".into()),
                    title: "Dragon Age™: Inquisition".into(),
                },
                OwnedOffer {
                    offer_id: "Origin.OFR.50.0000777".into(),
                    content_id: None,
                    title: "Mass Effect".into(),
                },
            ]
        );
        assert_eq!(http.urls().len(), 3);
    }

    #[test]
    fn rejected_tokens_and_errors_are_readable() {
        let http = FakeHttp::new(|_| response(401, "{}"));
        assert!(matches!(
            fetch_owned(&http, "tok"),
            Err(ProviderError::NotConfigured(_))
        ));
        let http =
            FakeHttp::new(|_| response(200, r#"{"errors":[{"message":"bad query"}],"data":null}"#));
        match fetch_owned(&http, "tok") {
            Err(ProviderError::Parse(m)) => assert!(m.contains("bad query")),
            other => panic!("{other:?}"),
        }
        let http = FakeHttp::new(|_| {
            response(200, r#"{"data":{"me":{"player":{"displayName":" Ann "}}}}"#)
        });
        assert_eq!(fetch_display_name(&http, "tok").as_deref(), Some("Ann"));
        let http = FakeHttp::new(|_| Err("offline".into()));
        assert_eq!(fetch_display_name(&http, "tok"), None);
    }

    #[test]
    fn checks_token_shape() {
        assert!(plausible_token("QVQwOjIuMDozLjA6NjA6abc+/="));
        assert!(!plausible_token("short"));
        assert!(!plausible_token("has space inside the token 123"));
    }
}
