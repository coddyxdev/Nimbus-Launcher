//! Microsoft / Xbox Live / Minecraft authentication via the OAuth 2.0 device
//! code flow.
//!
//! Why device code and not the authorization code flow: a desktop launcher
//! would otherwise have to spin up a throwaway HTTP server and register a
//! redirect URI. Device code needs neither — the user gets a short code, types
//! it on a Microsoft page in their browser, and we poll for the result.
//!
//! The full chain is four hops, each one refusing to work without the previous:
//!   1. Microsoft OAuth  → MS access token (scope `XboxLive.signin`)
//!   2. Xbox Live        → XBL token + user hash
//!   3. XSTS             → XSTS token for `rp://api.minecraftservices.com/`
//!   4. Minecraft        → the access token the game actually receives
//!
//! Two hard requirements that produce unhelpful errors when violated:
//!   - the `consumers` tenant must be used (not `common`, not a tenant id), so
//!     only personal Microsoft accounts can sign in;
//!   - the Azure application must be approved for the Minecraft API, otherwise
//!     step 4 answers 403 while steps 1-3 succeed. See `AZURE_SETUP.md`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::download::client;
use crate::error::{NimbusError, Result};

const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

/// Requesting anything beyond these two scopes makes the Xbox step fail.
const SCOPE: &str = "XboxLive.signin offline_access";

/// Client id Nimbus ships with, so an ordinary user only presses "sign in with
/// Microsoft" instead of registering an Azure application of their own.
///
/// Empty means this build has none and sign-in stays unavailable until the user
/// supplies an id in settings.
pub const BUILT_IN_CLIENT_ID: &str = "71bf4de7-3c1f-4569-8f9a-da526fb5208b";

/// The client id to sign in with: the user's own Azure application when they
/// configured one, otherwise the built-in id.
pub fn resolve_client_id(user_override: Option<&str>) -> Result<String> {
    let candidate = user_override
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(BUILT_IN_CLIENT_ID)
        .trim();

    if candidate.is_empty() {
        return Err(NimbusError::Invalid(
            "Вход через Microsoft недоступен: в сборке нет Azure Client ID, укажите свой в настройках"
                .to_owned(),
        ));
    }
    Ok(candidate.to_owned())
}

/// True when sign-in can be attempted at all, so the UI knows whether to offer
/// the button.
pub fn sign_in_available(user_override: Option<&str>) -> bool {
    resolve_client_id(user_override).is_ok()
}

/// Renew a Minecraft token this long before it actually expires, so a launch
/// never races the expiry.
const REFRESH_MARGIN_SECS: u64 = 300;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn net(err: impl std::fmt::Display) -> NimbusError {
    NimbusError::Network(err.to_string())
}

// ─── Step 1: device code ────────────────────────────────────────────────────

/// What the user has to be shown to start signing in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    /// The short code the user types on the Microsoft page.
    pub user_code: String,
    /// Page to open (`https://microsoft.com/link` in practice).
    pub verification_uri: String,
    /// Opaque handle we poll with. Never shown to the user.
    pub device_code: String,
    /// Seconds until the code dies.
    pub expires_in: u64,
    /// Minimum seconds between polls, as dictated by the server.
    pub interval: u64,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    user_code: String,
    device_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// Asks Microsoft for a device code. Fails fast when the client id is empty or
/// not allowed to use the public device flow.
pub async fn request_device_code(client_id: &str) -> Result<DeviceCode> {
    let client_id = client_id.trim();
    if client_id.is_empty() {
        return Err(NimbusError::Invalid(
            "Не задан Azure Client ID для входа через Microsoft".to_owned(),
        ));
    }

    let response = client()
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await
        .map_err(net)?;

    let status = response.status();
    let body = response.text().await.map_err(net)?;
    if !status.is_success() {
        return Err(NimbusError::Invalid(format!(
            "Microsoft отклонил запрос кода ({}): {}",
            status.as_u16(),
            describe_oauth_error(&body)
        )));
    }

    let parsed: DeviceCodeResponse = serde_json::from_str(&body)?;
    Ok(DeviceCode {
        user_code: parsed.user_code,
        verification_uri: parsed.verification_uri,
        device_code: parsed.device_code,
        expires_in: parsed.expires_in,
        interval: parsed.interval.max(1),
    })
}

// ─── Step 2: polling for the Microsoft token ────────────────────────────────

/// Microsoft tokens. The refresh token is what lets us re-authenticate later
/// without asking the user for the code again.
#[derive(Debug, Clone)]
pub struct MsTokens {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: String,
}

/// Turns an OAuth error body into something a user can act on.
fn describe_oauth_error(body: &str) -> String {
    match serde_json::from_str::<OAuthError>(body) {
        Ok(err) => match err.error.as_str() {
            "invalid_client" | "unauthorized_client" => {
                "приложение не найдено или ему запрещён публичный вход. Проверьте Client ID \
                 и включите «Allow public client flows» в Azure"
                    .to_owned()
            }
            "invalid_scope" => {
                "приложению не разрешён доступ XboxLive.signin".to_owned()
            }
            _ if !err.error_description.is_empty() => {
                // Microsoft descriptions are multi-line; the first line is enough.
                err.error_description
                    .lines()
                    .next()
                    .unwrap_or(&err.error)
                    .to_owned()
            }
            _ => err.error,
        },
        Err(_) => body.chars().take(200).collect(),
    }
}

/// Outcome of a single poll, so the caller can drive its own loop.
enum PollOutcome {
    Pending,
    SlowDown,
    Done(MsTokens),
}

async fn poll_once(client_id: &str, device_code: &str) -> Result<PollOutcome> {
    let response = client()
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id),
            ("device_code", device_code),
        ])
        .send()
        .await
        .map_err(net)?;

    let status = response.status();
    let body = response.text().await.map_err(net)?;

    if status.is_success() {
        let parsed: TokenResponse = serde_json::from_str(&body)?;
        if parsed.refresh_token.is_empty() {
            return Err(NimbusError::Invalid(
                "Microsoft не выдал refresh token — добавьте scope offline_access".to_owned(),
            ));
        }
        return Ok(PollOutcome::Done(MsTokens {
            access_token: parsed.access_token,
            refresh_token: parsed.refresh_token,
        }));
    }

    let code = serde_json::from_str::<OAuthError>(&body)
        .map(|e| e.error)
        .unwrap_or_default();

    match code.as_str() {
        "authorization_pending" => Ok(PollOutcome::Pending),
        "slow_down" => Ok(PollOutcome::SlowDown),
        "authorization_declined" => Err(NimbusError::Invalid(
            "Вход отменён в браузере".to_owned(),
        )),
        "expired_token" => Err(NimbusError::Invalid(
            "Код истёк — начните вход заново".to_owned(),
        )),
        "bad_verification_code" => Err(NimbusError::Invalid(
            "Код входа недействителен — начните заново".to_owned(),
        )),
        _ => Err(NimbusError::Invalid(format!(
            "Ошибка входа Microsoft: {}",
            describe_oauth_error(&body)
        ))),
    }
}

/// Server-mandated backoff (`slow_down`) is additive without an upper bound in
/// the OAuth device-flow spec; left unchecked a string of such responses can
/// stretch the poll interval to minutes.
const MAX_POLL_INTERVAL_SECS: u64 = 30;
/// Consecutive network failures tolerated before giving up. A single flaky
/// request must not abort a sign-in the user is actively completing in the
/// browser.
const MAX_CONSECUTIVE_POLL_NETWORK_ERRORS: u32 = 5;

/// Polls until the user finishes signing in, the code expires, or `cancelled`
/// starts returning true.
pub async fn await_device_token(
    client_id: &str,
    device: &DeviceCode,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<MsTokens> {
    let deadline = now_secs() + device.expires_in;
    let mut interval = device.interval;
    let mut network_errors = 0u32;

    loop {
        if cancelled() {
            return Err(NimbusError::Cancelled);
        }
        if now_secs() >= deadline {
            return Err(NimbusError::Invalid(
                "Время на ввод кода истекло — начните вход заново".to_owned(),
            ));
        }

        // Poll immediately on every pass (including the first) instead of
        // sleeping `interval` seconds before checking even once, which used
        // to add a needless wait to the start of every sign-in.
        match poll_once(client_id, &device.device_code).await {
            Ok(PollOutcome::Done(tokens)) => return Ok(tokens),
            Ok(PollOutcome::Pending) => {
                network_errors = 0;
            }
            // The server asks us to back off; ignoring it risks a hard block,
            // but growth is capped so a misbehaving server cannot stretch
            // this to minutes.
            Ok(PollOutcome::SlowDown) => {
                interval = (interval + 5).min(MAX_POLL_INTERVAL_SECS);
                network_errors = 0;
            }
            // A flaky connection must not abort a sign-in the user is
            // actively completing in the browser; only give up after several
            // failures in a row.
            Err(err @ NimbusError::Network(_)) => {
                network_errors += 1;
                if network_errors > MAX_CONSECUTIVE_POLL_NETWORK_ERRORS {
                    return Err(err);
                }
            }
            Err(err) => return Err(err),
        }

        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

/// Exchanges a stored refresh token for a fresh access token.
pub async fn refresh_tokens(client_id: &str, refresh_token: &str) -> Result<MsTokens> {
    let response = client()
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await
        .map_err(net)?;

    let status = response.status();
    let body = response.text().await.map_err(net)?;
    if !status.is_success() {
        return Err(NimbusError::Invalid(format!(
            "Не удалось обновить вход Microsoft: {}",
            describe_oauth_error(&body)
        )));
    }

    let parsed: TokenResponse = serde_json::from_str(&body)?;
    Ok(MsTokens {
        access_token: parsed.access_token,
        // Microsoft may or may not rotate the refresh token.
        refresh_token: if parsed.refresh_token.is_empty() {
            refresh_token.to_owned()
        } else {
            parsed.refresh_token
        },
    })
}

/// Renews only the Minecraft session (XBL → XSTS → Minecraft login) for an
/// already-known account.
///
/// Used before each launch when the stored token is stale. `finish_login`
/// additionally re-checks entitlement ownership and re-fetches the profile —
/// two requests that cannot produce a different answer than the last full
/// sign-in, so skipping them here saves 2 of 5 requests on every launch with
/// an expired token.
/// Returns the Minecraft access token, its expiry, and the Xbox user id.
pub async fn refresh_minecraft_session(ms_access_token: &str) -> Result<(String, u64, String)> {
    let (xbl_token, _) = xbox_authenticate(ms_access_token).await?;
    let (xsts_token, uhs, xuid) = xsts_authorize(&xbl_token).await?;
    let mc = minecraft_login(&uhs, &xsts_token).await?;
    Ok((
        mc.access_token,
        now_secs() + mc.expires_in.max(3600),
        xuid,
    ))
}

// ─── Step 3: Xbox Live and XSTS ─────────────────────────────────────────────

#[derive(Deserialize)]
struct XboxResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
    /// Xbox user id. Present in the XSTS answer only (the plain Xbox Live step
    /// does not return it), and the modern client expects it as ${auth_xuid}.
    #[serde(default)]
    xid: String,
}

#[derive(Deserialize)]
struct XstsError {
    #[serde(rename = "XErr", default)]
    xerr: u64,
}

/// Exchanges the Microsoft token for an Xbox Live token and the user hash.
async fn xbox_authenticate(ms_access_token: &str) -> Result<(String, String)> {
    let payload = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            // The `d=` prefix is mandatory and silently breaks things if missing.
            "RpsTicket": format!("d={ms_access_token}"),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });

    let response = client()
        .post(XBL_URL)
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(net)?;

    if !response.status().is_success() {
        return Err(NimbusError::Invalid(format!(
            "Xbox Live отклонил вход ({})",
            response.status().as_u16()
        )));
    }

    let parsed: XboxResponse = response.json().await.map_err(net)?;
    let uhs = parsed
        .display_claims
        .xui
        .first()
        .map(|x| x.uhs.clone())
        .ok_or_else(|| NimbusError::Invalid("Xbox Live не вернул user hash".to_owned()))?;
    Ok((parsed.token, uhs))
}

/// Upgrades the Xbox token to an XSTS token scoped to Minecraft services.
/// Returns the token, the user hash and the Xbox user id (xuid).
async fn xsts_authorize(xbl_token: &str) -> Result<(String, String, String)> {
    let payload = serde_json::json!({
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });

    let response = client()
        .post(XSTS_URL)
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(net)?;

    let status = response.status();
    let body = response.text().await.map_err(net)?;

    if !status.is_success() {
        // XErr codes are the only way to tell these cases apart, and each one
        // needs a different action from the user.
        let xerr = serde_json::from_str::<XstsError>(&body)
            .map(|e| e.xerr)
            .unwrap_or(0);
        let message = match xerr {
            2_148_916_233 => {
                "У этого аккаунта Microsoft нет профиля Xbox. Создайте его на xbox.com и \
                 повторите вход"
            }
            2_148_916_235 => "Xbox Live недоступен в стране аккаунта",
            2_148_916_236 | 2_148_916_237 => {
                "Аккаунту требуется подтверждение для взрослых (Xbox adult verification)"
            }
            2_148_916_238 => {
                "Это детский аккаунт — его нужно добавить в семейную группу Microsoft"
            }
            _ => "Xbox Live отказал в авторизации",
        };
        return Err(NimbusError::Invalid(message.to_owned()));
    }

    let parsed: XboxResponse = serde_json::from_str(&body)?;
    let claims = parsed
        .display_claims
        .xui
        .first()
        .ok_or_else(|| NimbusError::Invalid("XSTS не вернул user hash".to_owned()))?;
    Ok((parsed.token, claims.uhs.clone(), claims.xid.clone()))
}

// ─── Step 4: Minecraft ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct McLoginResponse {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
}

#[derive(Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Entitlements {
    #[serde(default)]
    items: Vec<serde_json::Value>,
}

/// The signed-in Minecraft account, ready to be persisted.
#[derive(Debug, Clone)]
pub struct AuthenticatedAccount {
    pub uuid: String,
    pub name: String,
    /// Xbox user id, passed to the game as ${auth_xuid}.
    pub xuid: String,
    pub mc_access_token: String,
    /// Unix seconds at which the Minecraft token stops working.
    pub mc_expires_at: u64,
    pub ms_refresh_token: String,
}

async fn minecraft_login(uhs: &str, xsts_token: &str) -> Result<McLoginResponse> {
    let payload = serde_json::json!({
        "identityToken": format!("XBL3.0 x={uhs};{xsts_token}"),
    });

    let response = client()
        .post(MC_LOGIN_URL)
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(net)?;

    let status = response.status();
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err(NimbusError::Invalid(
            "Minecraft API отклонил приложение (403). Azure-приложение должно пройти \
             проверку по форме aka.ms/mce-reviewappid — см. AZURE_SETUP.md"
                .to_owned(),
        ));
    }
    if !status.is_success() {
        return Err(NimbusError::Invalid(format!(
            "Minecraft отклонил вход ({})",
            status.as_u16()
        )));
    }

    response.json().await.map_err(net)
}

/// True when the account actually owns Java Edition. A signed-in account
/// without an entitlement cannot launch, so this is checked before saving.
///
/// Only 403/404 are treated as a real "no entitlement" answer. Anything else
/// (5xx, other 4xx, a proxy timeout surfaced as a non-2xx status) is a
/// transient or unrelated failure, not proof the account lacks the game --
/// the caller must not wipe a valid account over it, so it is surfaced as an
/// error instead of a false `Ok(false)`.
async fn owns_game(mc_token: &str) -> Result<bool> {
    let response = client()
        .get(MC_ENTITLEMENTS_URL)
        .bearer_auth(mc_token)
        .send()
        .await
        .map_err(net)?;

    let status = response.status();
    if status.is_success() {
        let parsed: Entitlements = response.json().await.map_err(net)?;
        return Ok(!parsed.items.is_empty());
    }
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }
    Err(NimbusError::Http {
        status: status.as_u16(),
        url: MC_ENTITLEMENTS_URL.to_owned(),
        retriable: status.is_server_error(),
    })
}

async fn fetch_profile(mc_token: &str) -> Result<McProfileResponse> {
    let response = client()
        .get(MC_PROFILE_URL)
        .bearer_auth(mc_token)
        .send()
        .await
        .map_err(net)?;

    if !response.status().is_success() {
        return Err(NimbusError::Invalid(
            "Не удалось получить профиль Minecraft. Возможно, на аккаунте не создан игрок"
                .to_owned(),
        ));
    }
    response.json().await.map_err(net)
}

/// Runs steps 2-4 for an already obtained Microsoft token.
pub async fn finish_login(tokens: MsTokens) -> Result<AuthenticatedAccount> {
    let (xbl_token, _) = xbox_authenticate(&tokens.access_token).await?;
    let (xsts_token, uhs, xuid) = xsts_authorize(&xbl_token).await?;
    let mc = minecraft_login(&uhs, &xsts_token).await?;

    if !owns_game(&mc.access_token).await? {
        return Err(NimbusError::Invalid(
            "На этом аккаунте нет Minecraft: Java Edition".to_owned(),
        ));
    }

    let profile = fetch_profile(&mc.access_token).await?;
    // Mojang returns the UUID unhyphenated; the game accepts it as-is.
    Ok(AuthenticatedAccount {
        uuid: profile.id,
        name: profile.name,
        xuid,
        mc_access_token: mc.access_token,
        mc_expires_at: now_secs() + mc.expires_in.max(3600),
        ms_refresh_token: tokens.refresh_token,
    })
}

/// True when a stored Minecraft token is too close to expiry to be trusted.
pub fn token_stale(expires_at: u64) -> bool {
    now_secs() + REFRESH_MARGIN_SECS >= expires_at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xsts_claims_expose_the_xuid() {
        let body = r#"{"Token":"t","DisplayClaims":{"xui":[{"uhs":"hash","xid":"2535000000000000"}]}}"#;
        let parsed: XboxResponse = serde_json::from_str(body).unwrap();
        let claims = parsed.display_claims.xui.first().unwrap();
        assert_eq!(claims.uhs, "hash");
        assert_eq!(claims.xid, "2535000000000000");
    }

    #[test]
    fn xbox_live_answer_without_an_xid_still_parses() {
        // The Xbox Live step returns only the user hash; a missing xid must
        // not fail the whole sign-in.
        let body = r#"{"Token":"t","DisplayClaims":{"xui":[{"uhs":"hash"}]}}"#;
        let parsed: XboxResponse = serde_json::from_str(body).unwrap();
        assert!(parsed.display_claims.xui[0].xid.is_empty());
    }

    #[test]
    fn user_override_wins_over_built_in() {
        let id = resolve_client_id(Some("  1234-abcd  ")).expect("override is usable");
        assert_eq!(id, "1234-abcd");
    }

    #[test]
    fn blank_override_falls_back_to_built_in() {
        // A cleared field means "use the id shipped with the launcher", not
        // "sign-in is off".
        assert_eq!(
            resolve_client_id(Some("   ")).ok(),
            resolve_client_id(None).ok()
        );
    }

    #[test]
    fn availability_matches_resolution() {
        assert!(sign_in_available(Some("1234-abcd")));
        assert_eq!(
            sign_in_available(None),
            !BUILT_IN_CLIENT_ID.trim().is_empty()
        );
    }
}