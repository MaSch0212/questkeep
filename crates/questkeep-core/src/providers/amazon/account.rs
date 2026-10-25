//! Amazon Games account sign-in and the owned games ("entitlements").
//!
//! Flow (the one Playnite's Amazon library uses, re-implemented from its public facts):
//! 1. Amazon's login page (`www.amazon.com/ap/signin`) with the Amazon Games launcher's public
//!    device client id and a PKCE challenge. After signing in, Amazon redirects to
//!    `https://www.amazon.com/?...&openid.oa2.authorization_code=<code>`.
//! 2. `POST api.amazon.com/auth/register` registers this PC as a launcher device with the code and
//!    the PKCE verifier and returns a bearer access token and a refresh token. Only the refresh
//!    token is persisted (keyring).
//! 3. `POST api.amazon.com/auth/token` turns the refresh token into a new access token.
//! 4. `POST gaming.amazon.com/api/distribution/entitlements` (`GetEntitlements`, paged) lists the
//!    games of the account; Prime "Fuel" entitlements (in-game loot) are skipped.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::super::ProviderError;
use super::AMAZON_SOURCE;
use crate::auth::{
    AccountAuth, AccountSession, AccountStatus, AuthError, Clock, Http, HttpRequest, HttpResponse,
    TokenSet, TokenStore, UreqHttp, query_param, system_clock,
};
use crate::model::ImportedGame;

/// The public device client id of the Amazon Games launcher (hex-encoded); it identifies the
/// application, not the user.
const CLIENT_ID: &str = "3733646238643238366332613932346432653737653161663637373636363435234132554d56484f58375550345637";
const DEVICE_TYPE: &str = "A2UMVHOX7UP4V7";
const SIGNIN_URL: &str = "https://www.amazon.com/ap/signin";
const REGISTER_URL: &str = "https://api.amazon.com/auth/register";
const TOKEN_URL: &str = "https://api.amazon.com/auth/token";
const ENTITLEMENTS_URL: &str = "https://gaming.amazon.com/api/distribution/entitlements";
/// Where Amazon sends the browser after signing in; the code is in its query.
pub const LOGIN_RETURN_PREFIX: &str = "https://www.amazon.com";
/// The query parameter carrying the authorization code.
pub const CODE_PARAM: &str = "openid.oa2.authorization_code";
const LAUNCHER_USER_AGENT: &str = "com.amazon.agslauncher.win/3.0.9495.3";
/// A key id the launcher sends with every entitlements request.
const ENTITLEMENTS_KEY_ID: &str = "d5dc8b8b-86c8-4fc4-ae93-18c0def5314d";
const FUEL_PRODUCT_LINE: &str = "Twitch:FuelEntitlement";

/// The login page for a PKCE `challenge`. The parameters are the launcher's own.
pub fn login_url(challenge: &str) -> String {
    format!(
        "{SIGNIN_URL}?openid.ns=http://specs.openid.net/auth/2.0\
         &openid.claimed_id=http://specs.openid.net/auth/2.0/identifier_select\
         &openid.identity=http://specs.openid.net/auth/2.0/identifier_select\
         &openid.mode=checkid_setup&openid.oa2.scope=device_auth_access\
         &openid.ns.oa2=http://www.amazon.com/ap/ext/oauth/2&openid.oa2.response_type=code\
         &openid.oa2.code_challenge_method=S256&openid.oa2.client_id=device:{CLIENT_ID}\
         &language=en_US&marketPlaceId=ATVPDKIKX0DER&openid.return_to=https://www.amazon.com\
         &openid.pape.max_auth_age=0&openid.assoc_handle=amzn_sonic_games_launcher\
         &pageId=amzn_sonic_games_launcher&openid.oa2.code_challenge={challenge}"
    )
}

/// A new PKCE verifier: 64 random hex characters (two v4 UUIDs, from the OS random source).
fn new_verifier() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// The S256 challenge of a PKCE verifier.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Amazon codes are short URL-safe strings.
fn plausible_amazon_code(code: &str) -> Option<String> {
    let code = code.trim().trim_matches(['"', '\'']).trim();
    let ok = (8..=512).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    ok.then(|| code.to_string())
}

/// The code from what the user pasted: the whole address Amazon sent them to, or just the code.
pub fn extract_code(input: &str) -> Option<String> {
    let input = input.trim();
    match query_param(input, CODE_PARAM) {
        Some(code) => plausible_amazon_code(&code),
        None if !input.contains(['?', '=', '/']) => plausible_amazon_code(input),
        None => None,
    }
}

/// Amazon sends `expires_in` as a string in some answers and as a number in others.
fn seconds(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn unreadable() -> AuthError {
    AuthError::Parse("Amazon returned an unexpected sign-in response.".into())
}

/// Maps a device registration answer to tokens (account id and name from `customer_info`).
pub fn parse_registration(body: &str, now: i64) -> Result<TokenSet, AuthError> {
    let json: Value = serde_json::from_str(body).map_err(|_| unreadable())?;
    let response = json.get("response").ok_or_else(unreadable)?;
    if let Some(error) = response.get("error") {
        let message = text(error.get("message"))
            .unwrap_or_else(|| "Amazon did not accept the sign-in.".into());
        return Err(AuthError::Rejected(format!(
            "{message} Sign in again to get a new code."
        )));
    }
    let success = response.get("success").ok_or_else(unreadable)?;
    let bearer = success.pointer("/tokens/bearer").ok_or_else(unreadable)?;
    let access_token = text(bearer.get("access_token")).ok_or_else(unreadable)?;
    let refresh_token = text(bearer.get("refresh_token")).ok_or_else(unreadable)?;
    let customer = success.pointer("/extensions/customer_info");
    let display_name =
        customer.and_then(|c| text(c.get("given_name")).or_else(|| text(c.get("name"))));
    Ok(TokenSet {
        access_token,
        refresh_token,
        expires_at: now + seconds(bearer.get("expires_in")).unwrap_or(3600),
        account_id: customer.and_then(|c| text(c.get("user_id"))),
        display_name,
    })
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<Value>,
}

fn status_error(response: &HttpResponse, what: &str) -> AuthError {
    match response.status {
        400 | 401 | 403 => AuthError::Rejected(what.to_string()),
        status => AuthError::Network(format!("Amazon returned HTTP {status}")),
    }
}

/// `RefreshFn` for [`AccountSession`].
pub fn refresh_tokens(http: &dyn Http, old: &TokenSet, now: i64) -> Result<TokenSet, AuthError> {
    let body = json!({
        "app_name": "AGSLauncher",
        "app_version": "3.0.9495.3",
        "source_token": old.refresh_token,
        "requested_token_type": "access_token",
        "source_token_type": "refresh_token",
    });
    let request = HttpRequest::post_json(TOKEN_URL, body.to_string());
    let response = http.send(&request).map_err(AuthError::Network)?;
    if !response.ok() {
        return Err(status_error(
            &response,
            "Amazon ended the sign-in. Sign in again.",
        ));
    }
    let parsed: RefreshResponse = serde_json::from_str(&response.body).map_err(|_| unreadable())?;
    if parsed.access_token.is_empty() {
        return Err(unreadable());
    }
    Ok(TokenSet {
        access_token: parsed.access_token,
        refresh_token: old.refresh_token.clone(),
        expires_at: now + seconds(parsed.expires_in.as_ref()).unwrap_or(3600),
        account_id: old.account_id.clone(),
        display_name: old.display_name.clone(),
    })
}

/// Store artwork of an owned product (absolute URLs).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntitlementArt {
    pub icon: Option<String>,
    pub background: Option<String>,
}

/// One page of `GetEntitlements`.
pub struct EntitlementPage {
    /// The games with their store artwork.
    pub games: Vec<(ImportedGame, EntitlementArt)>,
    /// The token of the next page.
    pub next: Option<String>,
}

/// Reads one page of `GetEntitlements`.
pub fn parse_entitlements(body: &str) -> Result<EntitlementPage, ProviderError> {
    let unexpected =
        || ProviderError::Parse("Amazon returned an unexpected response for the game list.".into());
    let json: Value = serde_json::from_str(body).map_err(|_| unexpected())?;
    let entries = match json.get("entitlements") {
        Some(Value::Array(entries)) => entries.as_slice(),
        Some(Value::Null) | None if json.is_object() => &[],
        _ => return Err(unexpected()),
    };
    let mut games = Vec::new();
    for entry in entries {
        let Some(product) = entry.get("product") else {
            continue;
        };
        if text(product.get("productLine")).as_deref() == Some(FUEL_PRODUCT_LINE) {
            continue;
        }
        let (Some(id), Some(title)) = (text(product.get("id")), text(product.get("title"))) else {
            continue;
        };
        let name = super::app::clean_name(&title);
        if name.is_empty() {
            continue;
        }
        let detail = product.get("productDetail");
        let https = |u: Option<String>| u.filter(|u| u.starts_with("https://"));
        let art = EntitlementArt {
            icon: https(text(detail.and_then(|d| d.get("iconUrl")))),
            background: https(
                text(detail.and_then(|d| d.pointer("/details/backgroundUrl1")))
                    .or_else(|| text(detail.and_then(|d| d.pointer("/details/backgroundUrl2")))),
            ),
        };
        let mut game = ImportedGame::installed(id, name);
        game.installed = false;
        games.push((game, art));
    }
    Ok(EntitlementPage {
        games,
        next: text(json.get("nextToken")),
    })
}

/// A stable id for this PC, so signing in again re-registers the same device.
fn device_serial() -> String {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
        let guid = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(
                r"SOFTWARE\Microsoft\Cryptography",
                KEY_READ | KEY_WOW64_64KEY,
            )
            .and_then(|key| key.get_value::<String, _>("MachineGuid"));
        if let Ok(guid) = guid
            && let Ok(parsed) = uuid::Uuid::parse_str(guid.trim())
        {
            return parsed.simple().to_string().to_uppercase();
        }
    }
    uuid::Uuid::new_v4().simple().to_string().to_uppercase()
}

pub struct AmazonAccount {
    session: AccountSession,
    http: Arc<dyn Http>,
    /// The PKCE verifier of the login page in use; replaced after each successful sign-in so a
    /// code is never accepted twice.
    verifier: Mutex<String>,
    /// Artwork of the owned products from the last entitlements scan.
    art: Mutex<HashMap<String, EntitlementArt>>,
}

impl std::fmt::Debug for AmazonAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AmazonAccount").finish_non_exhaustive()
    }
}

impl AmazonAccount {
    /// Uses the real network and system clock.
    pub fn new(tokens: Arc<dyn TokenStore>) -> Self {
        Self::with(tokens, Arc::new(UreqHttp::new()), system_clock())
    }

    /// Injectable network and clock (tests).
    pub fn with(tokens: Arc<dyn TokenStore>, http: Arc<dyn Http>, clock: Clock) -> Self {
        Self {
            session: AccountSession::new(
                AMAZON_SOURCE,
                "Amazon Games",
                tokens,
                http.clone(),
                clock,
                refresh_tokens,
            ),
            http,
            verifier: Mutex::new(new_verifier()),
            art: Mutex::new(HashMap::new()),
        }
    }

    pub fn is_signed_in(&self) -> bool {
        self.session.is_signed_in()
    }

    fn current_verifier(&self) -> String {
        self.verifier
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Artwork remembered from the last owned-games scan.
    pub fn art(&self, id: &str) -> EntitlementArt {
        self.art
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// All games of the account, not installed (the app scan marks the installed ones).
    pub fn fetch_owned(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        // The launcher sends a random hardware hash per session.
        let hardware_hash = uuid::Uuid::new_v4().simple().to_string();
        let mut games: Vec<ImportedGame> = Vec::new();
        let mut art = HashMap::new();
        let mut next: Option<String> = None;
        // A runaway server answer must not loop forever: 100 pages of 500 is far beyond any library.
        for _ in 0..100 {
            let body = json!({
                "Operation": "GetEntitlements",
                "clientId": "Sonic",
                "syncPoint": 0,
                "nextToken": next,
                "maxResults": 500,
                "keyId": ENTITLEMENTS_KEY_ID,
                "hardwareHash": hardware_hash,
                "productIdFilter": null,
                "disableStateFilter": true,
            })
            .to_string();
            let response = self.session.send(&|token| {
                HttpRequest::post_json(ENTITLEMENTS_URL, body.clone())
                    .header("User-Agent", LAUNCHER_USER_AGENT)
                    .header(
                        "X-Amz-Target",
                        "com.amazon.animusdistributionservice.entitlement.AnimusEntitlementsService.GetEntitlements",
                    )
                    .header("x-amzn-token", token)
                    .header("Content-Encoding", "amz-1.0")
            })?;
            let page = parse_entitlements(&response.body)?;
            for (game, product_art) in page.games {
                if games
                    .iter()
                    .any(|g| g.source_game_id == game.source_game_id)
                {
                    continue;
                }
                art.insert(game.source_game_id.clone(), product_art);
                games.push(game);
            }
            match page.next {
                Some(token) if Some(&token) != next.as_ref() => next = Some(token),
                _ => break,
            }
        }
        *self.art.lock().unwrap_or_else(PoisonError::into_inner) = art;
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }
}

impl AccountAuth for AmazonAccount {
    fn status(&self) -> AccountStatus {
        AccountStatus {
            provider: AMAZON_SOURCE.into(),
            name: "Amazon Games".into(),
            signed_in: self.session.is_signed_in(),
            display_name: self.session.display_name(),
            supports_code_sign_in: true,
            code_url: Some(self.login_url()),
        }
    }

    fn login_url(&self) -> String {
        login_url(&challenge(&self.current_verifier()))
    }

    fn sign_in_with_code(&self, input: &str) -> Result<(), AuthError> {
        let code = extract_code(input).ok_or_else(|| {
            AuthError::Rejected(
                "No Amazon sign-in code found. Paste the whole address of the Amazon page you were sent to after signing in."
                    .into(),
            )
        })?;
        let verifier = self.current_verifier();
        let body = json!({
            "auth_data": {
                "use_global_authentication": false,
                "authorization_code": code,
                "code_verifier": verifier,
                "code_algorithm": "SHA-256",
                "client_id": CLIENT_ID,
                "client_domain": "DeviceLegacy",
            },
            "registration_data": {
                "app_name": "AGSLauncher for Windows",
                "app_version": "1.0.0",
                "device_model": "Windows",
                "device_serial": device_serial(),
                "device_type": DEVICE_TYPE,
                "domain": "Device",
                "os_version": "10.0.19044.0",
            },
            "requested_extensions": ["customer_info", "device_info"],
            "requested_token_type": ["bearer", "mac_dms"],
        });
        let request = HttpRequest::post_json(REGISTER_URL, body.to_string())
            .header("User-Agent", "AGSLauncher/1.0.0");
        let response = self.http.send(&request).map_err(AuthError::Network)?;
        if !response.ok() && response.status >= 500 {
            return Err(status_error(&response, ""));
        }
        let tokens = parse_registration(&response.body, self.session.now())?;
        self.session.sign_in(tokens)?;
        *self.verifier.lock().unwrap_or_else(PoisonError::into_inner) = new_verifier();
        Ok(())
    }

    fn sign_out(&self) -> Result<(), AuthError> {
        self.art
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.session.sign_out()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;
    use crate::auth::MemoryTokenStore;
    use crate::auth::testing::{FakeHttp, response};

    const REGISTERED: &str = r#"{"response":{"success":{
        "tokens":{"bearer":{"access_token":"Atna|access","refresh_token":"Atnr|refresh","expires_in":"3600"},
                  "mac_dms":{"device_private_key":"x"}},
        "extensions":{"customer_info":{"user_id":"amzn1.account.ABC","given_name":"Ann","name":"Ann Smith"}}
    },"request_id":"r"}}"#;

    fn page(entries: &str, next: Option<&str>) -> String {
        let next = next.map_or("null".to_string(), |n| format!("\"{n}\""));
        format!(r#"{{"entitlements":[{entries}],"nextToken":{next}}}"#)
    }

    fn product(id: &str, title: &str, line: &str) -> String {
        format!(
            r#"{{"id":"e-{id}","state":"LIVE","product":{{"id":"{id}","title":"{title}","productLine":"{line}",
               "productDetail":{{"iconUrl":"https://m.media-amazon.com/{id}-icon.png",
               "details":{{"backgroundUrl1":"https://m.media-amazon.com/{id}-bg.jpg"}}}}}}}}"#
        )
    }

    fn account(http: FakeHttp) -> (AmazonAccount, Arc<AtomicI64>) {
        let now = Arc::new(AtomicI64::new(1_000));
        let clock_now = now.clone();
        let account = AmazonAccount::with(
            Arc::new(MemoryTokenStore::new()),
            Arc::new(http),
            Arc::new(move || clock_now.load(Ordering::SeqCst)),
        );
        (account, now)
    }

    #[test]
    fn pkce_challenge_is_the_s256_of_the_verifier() {
        // RFC 7636, appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let v = new_verifier();
        assert_eq!(v.len(), 64);
        assert_ne!(v, new_verifier());
    }

    #[test]
    fn login_url_carries_the_challenge_and_launcher_client() {
        let url = login_url("CHALLENGE");
        assert!(url.starts_with("https://www.amazon.com/ap/signin?"));
        assert_eq!(
            query_param(&url, "openid.oa2.code_challenge").as_deref(),
            Some("CHALLENGE")
        );
        assert_eq!(
            query_param(&url, "openid.oa2.client_id"),
            Some(format!("device:{CLIENT_ID}"))
        );
        assert_eq!(
            query_param(&url, "openid.return_to").as_deref(),
            Some(LOGIN_RETURN_PREFIX)
        );
        assert!(!url.contains(char::is_whitespace));
    }

    #[test]
    fn extracts_codes_from_the_return_address_or_alone() {
        let url = "https://www.amazon.com/?openid.assoc_handle=amzn_sonic_games_launcher&openid.oa2.authorization_code=ANdNAVhyhqirUelHGEHA&openid.mode=id_res";
        assert_eq!(extract_code(url).as_deref(), Some("ANdNAVhyhqirUelHGEHA"));
        assert_eq!(
            extract_code(" ANdNAVhyhqirUelHGEHA ").as_deref(),
            Some("ANdNAVhyhqirUelHGEHA")
        );
        assert_eq!(
            extract_code("https://www.amazon.com/?openid.mode=cancel"),
            None
        );
        assert_eq!(extract_code("short"), None);
    }

    #[test]
    fn signs_in_registers_the_device_and_lists_games() {
        let http = FakeHttp::new(|req| {
            if req.url == REGISTER_URL {
                let body: Value = serde_json::from_str(&req.body).unwrap();
                assert_eq!(
                    body["auth_data"]["authorization_code"],
                    "ANdNAVhyhqirUelHGEHA"
                );
                let verifier = body["auth_data"]["code_verifier"].as_str().unwrap();
                assert_eq!(verifier.len(), 64);
                return response(200, REGISTERED);
            }
            assert_eq!(req.url, ENTITLEMENTS_URL);
            let token = req
                .headers
                .iter()
                .find(|(n, _)| n == "x-amzn-token")
                .unwrap();
            assert_eq!(token.1, "Atna|access");
            let body: Value = serde_json::from_str(&req.body).unwrap();
            if body["nextToken"].is_null() {
                response(
                    200,
                    &page(
                        &[
                            product("b-id", "Beta™ Game", "Sonic"),
                            product("loot", "Prime Loot", FUEL_PRODUCT_LINE),
                        ]
                        .join(","),
                        Some("page2"),
                    ),
                )
            } else {
                assert_eq!(body["nextToken"], "page2");
                response(
                    200,
                    &page(
                        &[
                            product("a-id", "Alpha", "Sonic"),
                            product("b-id", "Beta™ Game", "Sonic"),
                        ]
                        .join(","),
                        None,
                    ),
                )
            }
        });
        let (account, _) = account(http);
        let url = account.login_url();
        let verifier = account.current_verifier();
        assert_eq!(
            query_param(&url, "openid.oa2.code_challenge"),
            Some(challenge(&verifier))
        );
        account
            .sign_in_with_code(
                "https://www.amazon.com/?openid.oa2.authorization_code=ANdNAVhyhqirUelHGEHA",
            )
            .unwrap();
        // A used verifier is never offered again.
        assert_ne!(account.current_verifier(), verifier);
        let status = account.status();
        assert!(status.signed_in);
        assert_eq!(status.display_name.as_deref(), Some("Ann"));

        let games = account.fetch_owned().unwrap();
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "Beta Game"]);
        assert!(games.iter().all(|g| !g.installed));
        assert_eq!(
            account.art("a-id"),
            EntitlementArt {
                icon: Some("https://m.media-amazon.com/a-id-icon.png".into()),
                background: Some("https://m.media-amazon.com/a-id-bg.jpg".into()),
            }
        );

        account.sign_out().unwrap();
        assert!(!account.status().signed_in);
        assert_eq!(account.art("a-id"), EntitlementArt::default());
    }

    #[test]
    fn expired_access_tokens_are_refreshed() {
        let http = FakeHttp::new(|req| match req.url.as_str() {
            REGISTER_URL => response(200, REGISTERED),
            TOKEN_URL => {
                let body: Value = serde_json::from_str(&req.body).unwrap();
                assert_eq!(body["source_token"], "Atnr|refresh");
                response(
                    200,
                    r#"{"access_token":"Atna|fresh","expires_in":3600,"token_type":"bearer"}"#,
                )
            }
            _ => {
                let token = req
                    .headers
                    .iter()
                    .find(|(n, _)| n == "x-amzn-token")
                    .unwrap();
                assert_eq!(token.1, "Atna|fresh");
                response(200, &page("", None))
            }
        });
        let (account, now) = account(http);
        account.sign_in_with_code("ANdNAVhyhqirUelHGEHA").unwrap();
        now.store(1_000 + 7_200, Ordering::SeqCst);
        assert!(account.fetch_owned().unwrap().is_empty());
    }

    #[test]
    fn rejected_codes_and_refresh_tokens_are_readable() {
        let http = FakeHttp::new(|req| match req.url.as_str() {
            REGISTER_URL => response(
                400,
                r#"{"response":{"error":{"code":"InvalidValue","message":"The authorization code is invalid."}}}"#,
            ),
            _ => response(400, r#"{"error":"invalid_grant"}"#),
        });
        let (account, _) = account(http);
        let err = account
            .sign_in_with_code("ANdNAVhyhqirUelHGEHA")
            .unwrap_err();
        assert!(
            matches!(err, AuthError::Rejected(ref m) if m.contains("invalid")),
            "{err}"
        );
        assert!(!account.status().signed_in);

        let old = TokenSet {
            access_token: String::new(),
            refresh_token: "r".into(),
            expires_at: 0,
            account_id: None,
            display_name: None,
        };
        let http = FakeHttp::new(|_| response(400, "{}"));
        assert!(matches!(
            refresh_tokens(&http, &old, 0),
            Err(AuthError::Rejected(_))
        ));
    }

    #[test]
    fn unexpected_entitlement_answers_are_errors() {
        assert!(parse_entitlements("not json").is_err());
        assert!(parse_entitlements("[1]").is_err());
        let page = parse_entitlements(r#"{"entitlements":null}"#).unwrap();
        assert!(page.games.is_empty() && page.next.is_none());
    }
}
