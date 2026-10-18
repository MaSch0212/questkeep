//! Xbox account sign-in, the owned PC titles and their playtime.
//!
//! Flow (the one Playnite's Xbox library uses, re-implemented from its public facts):
//! 1. Microsoft account OAuth (`login.live.com`, authorization code, redirect to
//!    `oauth20_desktop.srf`) with the public client id below; scope `Xboxlive.signin
//!    Xboxlive.offline_access` yields a refresh token. Only that token is persisted (keyring).
//! 2. Xbox user token: `POST user.auth.xboxlive.com/user/authenticate` with `RpsTicket = d=<MS
//!    access token>`.
//! 3. XSTS token: `POST xsts.auth.xboxlive.com/xsts/authorize` (relying party
//!    `http://xboxlive.com`); its claims carry the user hash (`uhs`), the XUID (`xid`) and the
//!    gamertag (`gtg`).
//! 4. `GET titlehub.xboxlive.com/users/xuid(<xuid>)/titles/titlehistory/decoration/detail` lists
//!    the titles of the account; `POST userstats.xboxlive.com/batch` adds `MinutesPlayed`.
//!
//! Steps 2 and 3 are repeated per scan (two small requests) so no Xbox token is ever stored.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::ProviderError;
use super::XBOX_SOURCE;
use crate::auth::{
    AccountAuth, AccountSession, AccountStatus, AuthError, Clock, Http, HttpRequest, HttpResponse,
    TokenSet, TokenStore, UreqHttp, query_param, system_clock, url_encode,
};
use crate::model::ImportedGame;

/// The public client id of the Xbox app registration that open-source launchers use; it
/// identifies the client application, not the user.
const CLIENT_ID: &str = "38cd2fa8-66fd-4760-afb2-405eb65d5b0c";
const SCOPE: &str = "Xboxlive.signin Xboxlive.offline_access";
const REDIRECT_URI: &str = "https://login.live.com/oauth20_desktop.srf";
const AUTHORIZE_URL: &str = "https://login.live.com/oauth20_authorize.srf";
const TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";
const USER_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const USER_STATS_URL: &str = "https://userstats.xboxlive.com/batch";
/// Where the login redirect lands; the shell watches for navigations to this prefix.
pub const LOGIN_REDIRECT_PREFIX: &str = "https://login.live.com/oauth20_desktop.srf";
/// Titles per `userstats` request (the service rejects very large batches).
const STATS_BATCH: usize = 100;

pub fn login_url() -> String {
    format!(
        "{AUTHORIZE_URL}?client_id={CLIENT_ID}&response_type=code&approval_prompt=auto&scope={}&redirect_uri={}",
        url_encode(SCOPE),
        url_encode(REDIRECT_URI)
    )
}

/// Microsoft codes look like `M.C507_BAY.2.U.<guid>` and may contain `*`, `!` and `$`.
fn plausible_ms_code(code: &str) -> Option<String> {
    let code = code.trim().trim_matches(['"', '\'']).trim();
    let ok = (8..=2048).contains(&code.len())
        && code.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'*' | b'!' | b'$')
        });
    ok.then(|| code.to_string())
}

/// The code from what the user pasted: the whole `oauth20_desktop.srf?code=...` address or just
/// the code.
pub fn extract_code(input: &str) -> Option<String> {
    let input = input.trim();
    match query_param(input, "code") {
        Some(code) if input.contains('=') => plausible_ms_code(&code),
        _ => plausible_ms_code(input),
    }
}

fn token_request(grant: &[(&str, &str)]) -> HttpRequest {
    let mut form: Vec<(String, String)> = grant
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    form.push(("scope".into(), SCOPE.into()));
    form.push(("client_id".into(), CLIENT_ID.into()));
    form.push(("redirect_uri".into(), REDIRECT_URI.into()));
    HttpRequest::post_form(TOKEN_URL, form)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Maps a token (or refresh) response to tokens. `previous` supplies the refresh token and
/// account when the response omits them.
pub fn parse_token_response(
    body: &str,
    now: i64,
    previous: Option<&TokenSet>,
) -> Result<TokenSet, AuthError> {
    let unreadable =
        || AuthError::Parse("Microsoft returned an unexpected sign-in response.".into());
    let parsed: TokenResponse = serde_json::from_str(body).map_err(|_| unreadable())?;
    let refresh_token = parsed
        .refresh_token
        .filter(|t| !t.is_empty())
        .or_else(|| previous.map(|p| p.refresh_token.clone()))
        .ok_or_else(unreadable)?;
    if parsed.access_token.is_empty() {
        return Err(unreadable());
    }
    Ok(TokenSet {
        access_token: parsed.access_token,
        refresh_token,
        expires_at: now + parsed.expires_in.unwrap_or(3600),
        account_id: previous.and_then(|p| p.account_id.clone()),
        display_name: previous.and_then(|p| p.display_name.clone()),
    })
}

fn token_status_error(response: &HttpResponse, what: &str) -> AuthError {
    match response.status {
        400 | 401 | 403 => AuthError::Rejected(what.to_string()),
        status => AuthError::Network(format!("Microsoft returned HTTP {status}")),
    }
}

/// `RefreshFn` for [`AccountSession`]: the "access token" of the session is the Microsoft one.
pub fn refresh_tokens(http: &dyn Http, old: &TokenSet, now: i64) -> Result<TokenSet, AuthError> {
    let request = token_request(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old.refresh_token),
    ]);
    let response = http.send(&request).map_err(AuthError::Network)?;
    if !response.ok() {
        return Err(token_status_error(
            &response,
            "Microsoft no longer accepts the saved Xbox sign-in.",
        ));
    }
    parse_token_response(&response.body, now, Some(old))
}

/// Step 2: the Xbox user token request for a Microsoft access token.
pub fn user_token_request(access_token: &str) -> HttpRequest {
    let body = json!({
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={access_token}"),
        }
    });
    HttpRequest::post_json(USER_AUTH_URL, body.to_string())
        .header("x-xbl-contract-version", "1")
        .header("Accept", "application/json")
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XblToken {
    #[serde(default)]
    token: String,
    #[serde(default)]
    display_claims: Option<DisplayClaims>,
}

#[derive(Deserialize)]
struct DisplayClaims {
    #[serde(default)]
    xui: Vec<Xui>,
}

#[derive(Deserialize, Default)]
struct Xui {
    #[serde(default)]
    uhs: String,
    #[serde(default)]
    xid: String,
    #[serde(default)]
    gtg: String,
}

/// The Xbox user token (step 2 response).
pub fn parse_user_token(body: &str) -> Result<String, AuthError> {
    let parsed: XblToken = serde_json::from_str(body)
        .map_err(|_| AuthError::Parse("Xbox returned an unexpected sign-in response.".into()))?;
    if parsed.token.is_empty() {
        return Err(AuthError::Parse(
            "Xbox returned an unexpected sign-in response.".into(),
        ));
    }
    Ok(parsed.token)
}

/// Step 3: the XSTS request for a user token.
pub fn xsts_request(user_token: &str) -> HttpRequest {
    let body = json!({
        "RelyingParty": "http://xboxlive.com",
        "TokenType": "JWT",
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [user_token] }
    });
    HttpRequest::post_json(XSTS_URL, body.to_string())
        .header("x-xbl-contract-version", "1")
        .header("Accept", "application/json")
}

/// A short-lived Xbox Live authorization; never persisted and never logged.
#[derive(Clone)]
pub struct Xsts {
    token: String,
    user_hash: String,
    pub xuid: String,
    pub gamertag: Option<String>,
}

impl std::fmt::Debug for Xsts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Xsts")
            .field("xuid", &self.xuid)
            .field("gamertag", &self.gamertag)
            .finish_non_exhaustive()
    }
}

impl Xsts {
    fn authorize(&self, request: HttpRequest) -> HttpRequest {
        request
            .header(
                "Authorization",
                format!("XBL3.0 x={};{}", self.user_hash, self.token),
            )
            .header("x-xbl-contract-version", "2")
            .header("Accept-Language", "en-US")
            .header("Accept", "application/json")
    }
}

/// What a refused XSTS request means (`XErr` in the 401 body), in words for the user.
fn xsts_error_message(body: &str) -> String {
    let code = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("XErr").and_then(Value::as_u64));
    match code {
        Some(2_148_916_233) => "This Microsoft account has no Xbox profile yet. Create one at xbox.com, then sign in again.".into(),
        Some(2_148_916_235) => "Xbox Live is not available in the country or region of this account.".into(),
        Some(2_148_916_236 | 2_148_916_237) => "Xbox needs an adult verification for this account before it can sign in.".into(),
        Some(2_148_916_238) => "This is a child account. An adult has to add it to a Microsoft family group before it can sign in.".into(),
        Some(code) => format!("Xbox refused the sign-in (error {code})."),
        None => "Xbox refused the sign-in.".into(),
    }
}

/// The XSTS response (step 3), or a readable error for a refusal.
pub fn parse_xsts(response: &HttpResponse) -> Result<Xsts, AuthError> {
    if !response.ok() {
        return Err(match response.status {
            400 | 401 | 403 => AuthError::Rejected(xsts_error_message(&response.body)),
            status => AuthError::Network(format!("Xbox returned HTTP {status}")),
        });
    }
    let unreadable = || AuthError::Parse("Xbox returned an unexpected sign-in response.".into());
    let parsed: XblToken = serde_json::from_str(&response.body).map_err(|_| unreadable())?;
    let claims = parsed
        .display_claims
        .and_then(|c| c.xui.into_iter().next())
        .ok_or_else(unreadable)?;
    let valid_xuid = !claims.xid.is_empty() && claims.xid.bytes().all(|b| b.is_ascii_digit());
    if parsed.token.is_empty() || claims.uhs.is_empty() || !valid_xuid {
        return Err(unreadable());
    }
    Ok(Xsts {
        token: parsed.token,
        user_hash: claims.uhs,
        xuid: claims.xid,
        gamertag: Some(claims.gtg.trim().to_string()).filter(|g| !g.is_empty()),
    })
}

/// Steps 2 and 3 for a Microsoft access token.
fn authorize(http: &dyn Http, access_token: &str) -> Result<Xsts, AuthError> {
    let response = http
        .send(&user_token_request(access_token))
        .map_err(AuthError::Network)?;
    if !response.ok() {
        return Err(match response.status {
            400 | 401 | 403 => AuthError::Rejected(
                "Xbox did not accept the Microsoft sign-in. Sign in again.".into(),
            ),
            status => AuthError::Network(format!("Xbox returned HTTP {status}")),
        });
    }
    let user_token = parse_user_token(&response.body)?;
    parse_xsts(
        &http
            .send(&xsts_request(&user_token))
            .map_err(AuthError::Network)?,
    )
}

pub fn title_history_url(xuid: &str) -> String {
    format!(
        "https://titlehub.xboxlive.com/users/xuid({xuid})/titles/titlehistory/decoration/detail"
    )
}

/// One entry of the title history that matters here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Title {
    pub title_id: String,
    pub pfn: String,
    pub name: String,
    pub last_played: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTitle {
    #[serde(default)]
    title_id: Option<Value>,
    #[serde(default)]
    pfn: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    devices: Vec<String>,
    #[serde(default)]
    title_history: Option<RawHistory>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHistory {
    #[serde(default)]
    last_time_played: Option<String>,
}

fn value_to_string(value: Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// PC games of a title history response: titles with a package family name, type `Game` and the
/// `PC` device, one per package family (case-insensitive).
pub fn parse_title_history(body: &str) -> Result<Vec<Title>, ProviderError> {
    let unreadable =
        || ProviderError::Parse("Xbox returned an unexpected response for the game list.".into());
    let json: Value = serde_json::from_str(body).map_err(|_| unreadable())?;
    let raw = json.get("titles").cloned().ok_or_else(unreadable)?;
    let raw: Vec<RawTitle> = serde_json::from_value(raw).map_err(|_| unreadable())?;
    let mut titles: Vec<Title> = Vec::new();
    for item in raw {
        let (Some(pfn), Some(name)) = (item.pfn, item.name) else {
            continue;
        };
        let (pfn, name) = (pfn.trim().to_string(), name.trim().to_string());
        let is_pc_game = item.kind.as_deref() == Some("Game")
            && item.devices.iter().any(|d| d.eq_ignore_ascii_case("PC"));
        if pfn.is_empty()
            || name.is_empty()
            || !is_pc_game
            || !super::catalog::is_valid_pfn(&pfn)
            || titles.iter().any(|t| t.pfn.eq_ignore_ascii_case(&pfn))
        {
            continue;
        }
        let last_played = item
            .title_history
            .and_then(|h| h.last_time_played)
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc))
            .filter(|d| d.year() >= 2000);
        titles.push(Title {
            title_id: item.title_id.and_then(value_to_string).unwrap_or_default(),
            pfn,
            name,
            last_played,
        });
    }
    Ok(titles)
}

fn stats_request(xsts: &Xsts, title_ids: &[&str]) -> HttpRequest {
    let body = json!({
        "arrangebyfield": "xuid",
        "xuids": [xsts.xuid],
        "stats": title_ids
            .iter()
            .map(|id| json!({ "name": "MinutesPlayed", "titleid": id }))
            .collect::<Vec<_>>(),
    });
    xsts.authorize(HttpRequest::post_json(USER_STATS_URL, body.to_string()))
}

/// `MinutesPlayed` per title id from a `userstats` response; titles without a value are absent.
pub fn parse_minutes_played(body: &str) -> HashMap<String, u64> {
    let Ok(json) = serde_json::from_str::<Value>(body) else {
        return HashMap::new();
    };
    let stats = json
        .pointer("/statlistscollection/0/stats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    stats
        .iter()
        .filter_map(|stat| {
            let id = value_to_string(stat.get("titleid")?.clone())?;
            let minutes = match stat.get("value")? {
                Value::String(s) => s.trim().parse::<u64>().ok()?,
                Value::Number(n) => n.as_u64()?,
                _ => return None,
            };
            Some((id, minutes))
        })
        .collect()
}

/// The games for the titles, with playtime where known.
pub fn games_from_titles(titles: &[Title], minutes: &HashMap<String, u64>) -> Vec<ImportedGame> {
    let mut games: Vec<ImportedGame> = titles
        .iter()
        .map(|title| ImportedGame {
            installed: false,
            playtime_seconds: minutes
                .get(&title.title_id)
                .copied()
                .filter(|m| *m > 0)
                .map(|m| m * 60),
            last_played: title.last_played,
            ..ImportedGame::installed(title.pfn.clone(), title.name.clone())
        })
        .collect();
    games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
    games
}

/// An Xbox account: sign-in, status and the owned games.
pub struct XboxAccount {
    session: AccountSession,
    http: Arc<dyn Http>,
}

impl std::fmt::Debug for XboxAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XboxAccount").finish_non_exhaustive()
    }
}

impl XboxAccount {
    /// Uses the real network and system clock.
    pub fn new(tokens: Arc<dyn TokenStore>) -> Self {
        Self::with(tokens, Arc::new(UreqHttp::new()), system_clock())
    }

    /// Injectable network and clock (tests).
    pub fn with(tokens: Arc<dyn TokenStore>, http: Arc<dyn Http>, clock: Clock) -> Self {
        Self {
            session: AccountSession::new(
                XBOX_SOURCE,
                "Xbox",
                tokens,
                http.clone(),
                clock,
                refresh_tokens,
            ),
            http,
        }
    }

    pub fn is_signed_in(&self) -> bool {
        self.session.is_signed_in()
    }

    /// Fresh Xbox Live authorization for the signed-in account.
    fn xsts(&self) -> Result<Xsts, ProviderError> {
        let response = self.session.send(&user_token_request)?;
        let user_token = parse_user_token(&response.body)?;
        let xsts = self
            .http
            .send(&xsts_request(&user_token))
            .map_err(ProviderError::Network)?;
        Ok(parse_xsts(&xsts)?)
    }

    /// The owned PC titles as not-installed games, with playtime and last played when known.
    pub fn fetch_owned(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        let xsts = self.xsts()?;
        let history = self
            .http
            .send(&xsts.authorize(HttpRequest::get(title_history_url(&xsts.xuid))))
            .map_err(ProviderError::Network)?;
        if !history.ok() {
            return Err(match history.status {
                401 | 403 => {
                    ProviderError::NotConfigured("Sign in to Xbox again in Settings.".into())
                }
                status => ProviderError::Network(format!("Xbox returned HTTP {status}")),
            });
        }
        let titles = parse_title_history(&history.body)?;
        let minutes = self.fetch_minutes(&xsts, &titles);
        Ok(games_from_titles(&titles, &minutes))
    }

    /// Playtime is a bonus: failures are logged and the library is imported without it.
    fn fetch_minutes(&self, xsts: &Xsts, titles: &[Title]) -> HashMap<String, u64> {
        let ids: Vec<&str> = titles
            .iter()
            .map(|t| t.title_id.as_str())
            .filter(|id| !id.is_empty())
            .collect();
        let mut minutes = HashMap::new();
        for chunk in ids.chunks(STATS_BATCH) {
            match self.http.send(&stats_request(xsts, chunk)) {
                Ok(response) if response.ok() => {
                    minutes.extend(parse_minutes_played(&response.body))
                }
                Ok(response) => {
                    tracing::warn!(
                        status = response.status,
                        "Xbox playtime request was refused"
                    );
                }
                Err(err) => tracing::warn!(%err, "Xbox playtime request failed"),
            }
        }
        minutes
    }
}

impl AccountAuth for XboxAccount {
    fn status(&self) -> AccountStatus {
        AccountStatus {
            provider: XBOX_SOURCE.into(),
            name: "Xbox".into(),
            signed_in: self.session.is_signed_in(),
            display_name: self.session.display_name(),
            supports_code_sign_in: true,
            code_url: Some(login_url()),
        }
    }

    fn login_url(&self) -> String {
        login_url()
    }

    fn sign_in_with_code(&self, input: &str) -> Result<(), AuthError> {
        let code = extract_code(input).ok_or_else(|| {
            AuthError::Rejected(
                "No Xbox sign-in code found. Paste the code or the whole address of the blank page you were sent to after signing in."
                    .into(),
            )
        })?;
        let request = token_request(&[("grant_type", "authorization_code"), ("code", &code)]);
        let response = self.http.send(&request).map_err(AuthError::Network)?;
        if !response.ok() {
            return Err(token_status_error(
                &response,
                "Microsoft did not accept that code. Codes work once and expire quickly; sign in again to get a new one.",
            ));
        }
        let mut tokens = parse_token_response(&response.body, self.session.now(), None)?;
        // The Xbox profile has to exist for the account to be useful; fail here, not later.
        let xsts = authorize(self.http.as_ref(), &tokens.access_token)?;
        tokens.account_id = Some(xsts.xuid.clone());
        tokens.display_name = xsts.gamertag.clone();
        self.session.sign_in(tokens)
    }

    fn sign_out(&self) -> Result<(), AuthError> {
        self.session.sign_out()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::MemoryTokenStore;
    use crate::auth::testing::{FakeHttp, response};

    const TOKEN: &str = include_str!("../../../tests/fixtures/xbox/token.json");
    const USER_TOKEN: &str = include_str!("../../../tests/fixtures/xbox/user_token.json");
    const XSTS: &str = include_str!("../../../tests/fixtures/xbox/xsts.json");
    const XSTS_ERROR: &str = include_str!("../../../tests/fixtures/xbox/xsts_error.json");
    const HISTORY: &str = include_str!("../../../tests/fixtures/xbox/title_history.json");
    const STATS: &str = include_str!("../../../tests/fixtures/xbox/user_stats.json");

    const CODE_URL: &str =
        "https://login.live.com/oauth20_desktop.srf?code=M.C507_BAY.2.U.0123abcd-4567&lc=1033";

    #[test]
    fn login_url_matches_the_playnite_flow() {
        assert_eq!(
            login_url(),
            "https://login.live.com/oauth20_authorize.srf?client_id=38cd2fa8-66fd-4760-afb2-405eb65d5b0c&response_type=code&approval_prompt=auto&scope=Xboxlive.signin%20Xboxlive.offline_access&redirect_uri=https%3A%2F%2Flogin.live.com%2Foauth20_desktop.srf"
        );
        assert!(LOGIN_REDIRECT_PREFIX.starts_with("https://login.live.com/"));
    }

    #[test]
    fn extracts_codes_from_urls_and_pastes() {
        assert_eq!(
            extract_code(CODE_URL).as_deref(),
            Some("M.C507_BAY.2.U.0123abcd-4567")
        );
        assert_eq!(
            extract_code("  M.C507_BAY.2.U.0123abcd*!$ \n").as_deref(),
            Some("M.C507_BAY.2.U.0123abcd*!$")
        );
        assert_eq!(
            extract_code("code=M.C507_BAY.2.U.0123abcd").as_deref(),
            Some("M.C507_BAY.2.U.0123abcd")
        );
        assert_eq!(
            extract_code("https://login.live.com/oauth20_desktop.srf?error=access_denied"),
            None
        );
        assert_eq!(extract_code(""), None);
        assert_eq!(extract_code("two words here"), None);
    }

    #[test]
    fn parses_token_response() {
        let tokens = parse_token_response(TOKEN, 1_000, None).unwrap();
        assert_eq!(tokens.access_token, "ms-access-token-value");
        assert_eq!(tokens.refresh_token, "ms-refresh-token-value");
        assert_eq!(tokens.expires_at, 1_000 + 86_400);
        assert!(parse_token_response("{}", 0, None).is_err());
        assert!(parse_token_response("<html>", 0, None).is_err());
        let prev = TokenSet {
            access_token: String::new(),
            refresh_token: "old-refresh".into(),
            expires_at: 0,
            account_id: Some("2535".into()),
            display_name: Some("Tag".into()),
        };
        let kept = parse_token_response(r#"{"access_token":"a"}"#, 0, Some(&prev)).unwrap();
        assert_eq!(kept.refresh_token, "old-refresh");
        assert_eq!(kept.account_id.as_deref(), Some("2535"));
        assert_eq!(kept.display_name.as_deref(), Some("Tag"));
    }

    #[test]
    fn xbox_requests_have_the_documented_shape() {
        let user = user_token_request("MSTOKEN123");
        assert_eq!(user.url, USER_AUTH_URL);
        let body: Value = serde_json::from_str(&user.body).unwrap();
        assert_eq!(body["RelyingParty"], "http://auth.xboxlive.com");
        assert_eq!(body["Properties"]["AuthMethod"], "RPS");
        assert_eq!(body["Properties"]["SiteName"], "user.auth.xboxlive.com");
        assert_eq!(body["Properties"]["RpsTicket"], "d=MSTOKEN123");
        assert!(!format!("{user:?}").contains("MSTOKEN123"));

        let xsts = xsts_request("USERTOKEN1");
        assert_eq!(xsts.url, XSTS_URL);
        let body: Value = serde_json::from_str(&xsts.body).unwrap();
        assert_eq!(body["RelyingParty"], "http://xboxlive.com");
        assert_eq!(body["Properties"]["SandboxId"], "RETAIL");
        assert_eq!(body["Properties"]["UserTokens"][0], "USERTOKEN1");
    }

    #[test]
    fn parses_xbox_tokens() {
        assert_eq!(
            parse_user_token(USER_TOKEN).unwrap(),
            "xbl-user-token-value"
        );
        assert!(parse_user_token("{}").is_err());
        let xsts = parse_xsts(&response(200, XSTS).unwrap()).unwrap();
        assert_eq!(xsts.xuid, "2535400000000001");
        assert_eq!(xsts.gamertag.as_deref(), Some("Quest Keeper"));
        let request = xsts.authorize(HttpRequest::get("https://x.test"));
        assert_eq!(
            request.headers[0],
            (
                "Authorization".to_string(),
                "XBL3.0 x=1234567890123456789;xsts-token-value".to_string()
            )
        );
        assert!(!format!("{xsts:?}").contains("xsts-token-value"));
        // A xuid that is not numeric never ends up in a URL.
        let bad = XSTS.replace("2535400000000001", "1)/../x");
        assert!(parse_xsts(&response(200, &bad).unwrap()).is_err());
    }

    #[test]
    fn xsts_refusals_are_readable() {
        let err = parse_xsts(&response(401, XSTS_ERROR).unwrap()).unwrap_err();
        assert!(matches!(&err, AuthError::Rejected(m) if m.contains("no Xbox profile")));
        let err = parse_xsts(&response(401, r#"{"XErr":2148916238}"#).unwrap()).unwrap_err();
        assert!(err.to_string().contains("child account"));
        let err = parse_xsts(&response(401, "").unwrap()).unwrap_err();
        assert_eq!(err.to_string(), "Xbox refused the sign-in.");
        assert!(matches!(
            parse_xsts(&response(503, "").unwrap()),
            Err(AuthError::Network(_))
        ));
    }

    #[test]
    fn title_history_keeps_pc_games_once() {
        let titles = parse_title_history(HISTORY).unwrap();
        let names: Vec<_> = titles.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Halo Infinite", "Racer"]);
        assert_eq!(titles[0].pfn, "Microsoft.254428597CFE2_8wekyb3d8bbwe");
        assert_eq!(titles[0].title_id, "2043073184");
        assert_eq!(
            titles[0].last_played.map(|d| d.to_rfc3339()),
            Some("2026-10-10T18:30:00.123456700+00:00".to_string())
        );
        assert_eq!(titles[1].last_played, None, "year 1 means never played");
        assert!(parse_title_history("{}").is_err());
        assert!(parse_title_history("nope").is_err());
        assert!(parse_title_history(r#"{"titles":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn minutes_become_playtime() {
        let minutes = parse_minutes_played(STATS);
        assert_eq!(minutes.get("2043073184"), Some(&125));
        assert!(parse_minutes_played("nope").is_empty());
        let games = games_from_titles(&parse_title_history(HISTORY).unwrap(), &minutes);
        assert_eq!(games[0].name, "Halo Infinite");
        assert_eq!(games[0].playtime_seconds, Some(125 * 60));
        assert!(games[0].last_played.is_some());
        assert_eq!(
            games[1].playtime_seconds, None,
            "zero minutes is no playtime"
        );
        assert!(
            games
                .iter()
                .all(|g| !g.installed && g.play_action.is_none())
        );
    }

    /// Everything the real service would be asked, answered from fixtures.
    fn happy_path(req: &HttpRequest) -> Result<HttpResponse, String> {
        match req.url.as_str() {
            TOKEN_URL => response(200, TOKEN),
            USER_AUTH_URL => response(200, USER_TOKEN),
            XSTS_URL => response(200, XSTS),
            USER_STATS_URL => response(200, STATS),
            url if url
                .starts_with("https://titlehub.xboxlive.com/users/xuid(2535400000000001)/") =>
            {
                response(200, HISTORY)
            }
            _ => response(404, ""),
        }
    }

    fn make_account(
        handler: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static,
    ) -> (XboxAccount, Arc<MemoryTokenStore>, Arc<FakeHttp>) {
        let store = Arc::new(MemoryTokenStore::new());
        let http = Arc::new(FakeHttp::new(handler));
        let account = XboxAccount::with(store.clone(), http.clone(), Arc::new(|| 1_000));
        (account, store, http)
    }

    #[test]
    fn code_sign_in_runs_the_xbox_chain_and_stores_only_the_refresh_token() {
        let (account, store, http) = make_account(happy_path);
        assert!(!account.status().signed_in);
        account.sign_in_with_code(CODE_URL).unwrap();
        let status = account.status();
        assert!(status.signed_in && status.supports_code_sign_in);
        assert_eq!(status.display_name.as_deref(), Some("Quest Keeper"));
        assert_eq!(status.code_url.as_deref(), Some(login_url().as_str()));
        let stored = store.load("xbox").unwrap().unwrap();
        assert!(stored.contains("ms-refresh-token-value"));
        assert!(stored.contains("2535400000000001"));
        for secret in [
            "ms-access-token-value",
            "xsts-token-value",
            "xbl-user-token-value",
        ] {
            assert!(!stored.contains(secret), "{secret} must not be persisted");
        }
        assert_eq!(http.urls(), [TOKEN_URL, USER_AUTH_URL, XSTS_URL]);
        let log = http.log.lock().unwrap();
        let form: HashMap<_, _> = log[0].form.iter().cloned().collect();
        assert_eq!(form["grant_type"], "authorization_code");
        assert_eq!(form["code"], "M.C507_BAY.2.U.0123abcd-4567");
        assert_eq!(form["redirect_uri"], REDIRECT_URI);
        assert_eq!(form["client_id"], CLIENT_ID);
        assert_eq!(form["scope"], SCOPE);
        drop(log);

        account.sign_out().unwrap();
        assert!(!account.status().signed_in);
        assert_eq!(store.load("xbox").unwrap(), None);
    }

    #[test]
    fn sign_in_errors_are_readable_and_secret_free() {
        let (account, store, _) = make_account(|_| response(400, r#"{"error":"invalid_grant"}"#));
        let err = account.sign_in_with_code(CODE_URL).unwrap_err();
        assert!(matches!(&err, AuthError::Rejected(m) if m.contains("did not accept")));
        assert!(!err.to_string().contains("0123abcd"));
        let err = account.sign_in_with_code("nonsense words").unwrap_err();
        assert!(err.to_string().contains("No Xbox sign-in code"));
        let (account, _, _) = make_account(|_| response(503, ""));
        assert!(matches!(
            account.sign_in_with_code(CODE_URL),
            Err(AuthError::Network(_))
        ));
        // No Xbox profile: nothing is stored.
        let (account, store2, _) = make_account(|req| match req.url.as_str() {
            TOKEN_URL => response(200, TOKEN),
            USER_AUTH_URL => response(200, USER_TOKEN),
            _ => response(401, XSTS_ERROR),
        });
        let err = account.sign_in_with_code(CODE_URL).unwrap_err();
        assert!(err.to_string().contains("no Xbox profile"));
        assert!(!account.status().signed_in);
        assert_eq!(store.load("xbox").unwrap(), None);
        assert_eq!(store2.load("xbox").unwrap(), None);
    }

    #[test]
    fn refresh_uses_the_refresh_grant() {
        let http = FakeHttp::new(|req| {
            let form: HashMap<_, _> = req.form.iter().cloned().collect();
            assert_eq!(form["grant_type"], "refresh_token");
            assert_eq!(form["refresh_token"], "R/1");
            response(200, TOKEN)
        });
        let old = TokenSet {
            access_token: String::new(),
            refresh_token: "R/1".into(),
            expires_at: 0,
            account_id: Some("2535".into()),
            display_name: Some("Tag".into()),
        };
        let fresh = refresh_tokens(&http, &old, 10).unwrap();
        assert_eq!(fresh.refresh_token, "ms-refresh-token-value");
        assert_eq!(fresh.display_name.as_deref(), Some("Tag"));
        let rejected = FakeHttp::new(|_| response(400, ""));
        assert!(matches!(
            refresh_tokens(&rejected, &old, 10),
            Err(AuthError::Rejected(_))
        ));
        let down = FakeHttp::new(|_| response(502, ""));
        assert!(matches!(
            refresh_tokens(&down, &old, 10),
            Err(AuthError::Network(_))
        ));
    }

    #[test]
    fn owned_scan_lists_games_with_playtime() {
        let (account, store, http) = make_account(happy_path);
        // A restored session (no access token yet) refreshes first.
        store
            .save("xbox", r#"{"refreshToken":"ms-refresh-token-value"}"#)
            .unwrap();
        let games = account.fetch_owned().unwrap();
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Halo Infinite", "Racer"]);
        assert_eq!(games[0].playtime_seconds, Some(7_500));
        let urls = http.urls();
        assert_eq!(urls[0], TOKEN_URL, "refresh before the first request");
        assert!(urls.iter().any(|u| u == USER_STATS_URL));
        let log = http.log.lock().unwrap();
        let history = log
            .iter()
            .find(|r| r.url.starts_with("https://titlehub"))
            .unwrap();
        assert!(
            history
                .url
                .ends_with("/titles/titlehistory/decoration/detail")
        );
        assert!(
            history.headers.iter().any(
                |(n, v)| n == "Authorization" && v.starts_with("XBL3.0 x=1234567890123456789;")
            )
        );
    }

    #[test]
    fn owned_scan_survives_a_playtime_failure_and_reports_real_failures() {
        let (account, store, _) = make_account(|req| {
            if req.url == USER_STATS_URL {
                response(500, "")
            } else {
                happy_path(req)
            }
        });
        store
            .save("xbox", r#"{"refreshToken":"ms-refresh-token-value"}"#)
            .unwrap();
        let games = account.fetch_owned().unwrap();
        assert_eq!(games.len(), 2);
        assert!(games.iter().all(|g| g.playtime_seconds.is_none()));

        let (account, store, _) = make_account(|req| {
            if req.url.starts_with("https://titlehub") {
                response(503, "")
            } else {
                happy_path(req)
            }
        });
        store
            .save("xbox", r#"{"refreshToken":"ms-refresh-token-value"}"#)
            .unwrap();
        assert!(matches!(
            account.fetch_owned(),
            Err(ProviderError::Network(_))
        ));
    }

    #[test]
    fn owned_scan_when_signed_out_is_not_configured() {
        let (account, _, _) = make_account(|_| response(200, ""));
        assert!(matches!(
            account.fetch_owned(),
            Err(ProviderError::NotConfigured(m)) if m == "Sign in to Xbox in Settings."
        ));
    }
}
