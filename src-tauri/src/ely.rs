//! Ely.by authentication: a free account service built specifically for
//! players without a Microsoft-owned Minecraft licence.
//!
//! Login speaks the classic Mojang "Yggdrasil" username+password protocol --
//! the one the official launcher itself used before the Microsoft account
//! migration -- because Ely.by deliberately keeps that protocol alive for
//! exactly this purpose: any launcher, including the unmodified vanilla
//! game, can authenticate against it with zero code changes on Mojang's
//! side.
//!
//! Signing in only gets a session, though. For the *game* to actually show
//! Ely.by skins -- to yourself and to every other player whose client is
//! also configured for Ely.by -- the JVM needs `authlib-injector` as a Java
//! agent pointed at Ely.by's API (see `ely_injector.rs`). Without it, the
//! game still tries to validate sessions and fetch skins from Mojang, which
//! rejects an Ely.by token outright.

use serde::{Deserialize, Serialize};

use crate::download;
use crate::error::{NimbusError, Result};

const AUTH_SERVER: &str = "https://authserver.ely.by";

/// `authlib-injector` API root for Ely.by. Passed as the agent argument at
/// launch time so the whole Yggdrasil surface (login validation, session
/// join, skin lookups) resolves against Ely.by instead of Mojang.
pub const AUTHLIB_INJECTOR_API: &str = "https://authserver.ely.by/api/authlib-injector";

/// A signed-in Ely.by session: enough to keep playing without asking for the
/// password again, and to refresh before every launch.
#[derive(Debug, Clone)]
pub struct ElyTokens {
    pub access_token: String,
    pub client_token: String,
    pub uuid: String,
    pub name: String,
}

#[derive(Serialize)]
struct AuthAgent {
    name: &'static str,
    version: u32,
}

#[derive(Serialize)]
struct AuthenticateRequest<'a> {
    username: &'a str,
    password: &'a str,
    #[serde(rename = "clientToken")]
    client_token: &'a str,
    #[serde(rename = "requestUser")]
    request_user: bool,
    agent: AuthAgent,
}

#[derive(Serialize)]
struct RefreshRequest<'a> {
    #[serde(rename = "accessToken")]
    access_token: &'a str,
    #[serde(rename = "clientToken")]
    client_token: &'a str,
    #[serde(rename = "requestUser")]
    request_user: bool,
}

#[derive(Deserialize)]
struct YggdrasilResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "clientToken")]
    client_token: String,
    #[serde(default, rename = "selectedProfile")]
    selected_profile: Option<YggdrasilProfile>,
}

#[derive(Deserialize)]
struct YggdrasilProfile {
    id: String,
    name: String,
}

#[derive(Deserialize, Default)]
struct YggdrasilError {
    #[serde(default)]
    error: String,
    #[serde(default, rename = "errorMessage")]
    error_message: String,
}

fn net(e: reqwest::Error) -> NimbusError {
    NimbusError::Network(e.to_string())
}

/// A fresh, opaque per-install identifier Ely.by asks every Yggdrasil call to
/// carry. It has no meaning beyond "the same launcher install", so a random
/// value per sign-in is fine -- Ely.by hands back whichever one it wants
/// remembered on every response anyway.
fn random_client_token() -> String {
    format!("{:032x}", rand::random::<u128>())
}

fn describe_error(status: reqwest::StatusCode, body: &str) -> NimbusError {
    if status.is_server_error() {
        return NimbusError::Http {
            status: status.as_u16(),
            url: AUTH_SERVER.to_owned(),
            retriable: true,
        };
    }
    let detail = serde_json::from_str::<YggdrasilError>(body)
        .map(|e| {
            if !e.error_message.is_empty() {
                e.error_message
            } else {
                e.error
            }
        })
        .unwrap_or_else(|_| body.chars().take(200).collect());
    NimbusError::Invalid(format!("Ely.by отклонил запрос: {detail}"))
}

async fn call(path: &str, body: impl Serialize) -> Result<YggdrasilResponse> {
    let resp = download::client()
        .post(format!("{AUTH_SERVER}{path}"))
        .json(&body)
        .send()
        .await
        .map_err(net)?;
    let status = resp.status();
    let text = resp.text().await.map_err(net)?;
    if !status.is_success() {
        return Err(describe_error(status, &text));
    }
    Ok(serde_json::from_str(&text)?)
}

/// Signs in with an Ely.by username (or email) and password.
pub async fn authenticate(username: &str, password: &str) -> Result<ElyTokens> {
    let client_token = random_client_token();
    let parsed = call(
        "/auth/authenticate",
        AuthenticateRequest {
            username,
            password,
            client_token: &client_token,
            request_user: false,
            agent: AuthAgent {
                name: "Minecraft",
                version: 1,
            },
        },
    )
    .await?;
    let profile = parsed.selected_profile.ok_or_else(|| {
        NimbusError::Invalid("На этом аккаунте Ely.by нет профиля Minecraft".to_owned())
    })?;
    Ok(ElyTokens {
        access_token: parsed.access_token,
        client_token: parsed.client_token,
        uuid: profile.id,
        name: profile.name,
    })
}

/// Exchanges a stored session for a fresh one. Ely.by's Yggdrasil tokens
/// carry no expiry the client can check locally, so callers refresh
/// unconditionally before every launch rather than guessing when to.
pub async fn refresh(access_token: &str, client_token: &str) -> Result<ElyTokens> {
    let parsed = call(
        "/auth/refresh",
        RefreshRequest {
            access_token,
            client_token,
            request_user: false,
        },
    )
    .await?;
    let profile = parsed.selected_profile.ok_or_else(|| {
        NimbusError::Invalid("Профиль Ely.by не найден при обновлении сессии".to_owned())
    })?;
    Ok(ElyTokens {
        access_token: parsed.access_token,
        client_token: parsed.client_token,
        uuid: profile.id,
        name: profile.name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_tokens_are_not_all_identical() {
        // Weak sanity check against a broken/constant RNG path, not a
        // cryptographic property test.
        let a = random_client_token();
        let b = random_client_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn describes_forbidden_operation_with_its_message() {
        let body = r#"{"error":"ForbiddenOperationException","errorMessage":"Invalid credentials"}"#;
        let err = describe_error(reqwest::StatusCode::FORBIDDEN, body);
        assert!(err.to_string().contains("Invalid credentials"));
    }
}
