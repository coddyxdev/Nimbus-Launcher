//! Minecraft skins: real ones via Mojang's official API (for signed-in
//! Microsoft accounts) and locally-stored ones for offline profiles.
//!
//! The two are fundamentally different in reach. A Microsoft account's skin
//! is stored by Mojang and shown by *every* client everywhere, exactly like
//! the official launcher's own skin picker -- because it is the official
//! launcher's skin picker, just called from here instead.
//!
//! An offline profile has no Mojang account behind it, so its skin lives
//! only inside Nimbus: it is not part of the Minecraft protocol, and whether
//! other players on a given server see it depends entirely on that server
//! (many offline-mode servers restore skins for non-premium players via a
//! plugin, many do not) -- that choice belongs to the server, not to any
//! launcher.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::download;
use crate::error::{NimbusError, Result};
use crate::paths;

/// Mojang's official skin endpoint. `POST` (JSON with a URL, or multipart
/// with a file) sets the active skin; `DELETE {this}/active` resets it.
const MOJANG_SKIN_API: &str = "https://api.minecraftservices.com/minecraft/profile/skins";

/// Which of the two player models a skin is drawn for. Mojang calls these
/// "classic" (wide/Steve-style arms) and "slim" (narrow/Alex-style arms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SkinModel {
    #[default]
    Classic,
    Slim,
}

impl SkinModel {
    pub fn as_variant(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Slim => "slim",
        }
    }

    pub fn parse(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("slim") {
            Self::Slim
        } else {
            Self::Classic
        }
    }
}

/// Where locally-managed skin images (one per offline profile) live.
pub fn skins_dir() -> Result<PathBuf> {
    Ok(paths::root()?.join("skins"))
}

/// Generous for a 64x64 PNG (real skins are a handful of KB), but small
/// enough that a mistaken upload cannot fill the disk or the request body.
const MAX_SKIN_BYTES: u64 = 2 * 1024 * 1024;

/// Confirms `bytes` is a PNG shaped like a Minecraft skin (64x64, or the
/// legacy 64x32) without pulling in an image-decoding crate: only the fixed
/// IHDR header every PNG starts with is read.
pub fn validate_skin_png(bytes: &[u8]) -> Result<()> {
    const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || bytes[0..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return Err(NimbusError::Invalid(
            "Файл должен быть изображением скина в формате PNG".to_owned(),
        ));
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width != 64 || (height != 64 && height != 32) {
        return Err(NimbusError::Invalid(format!(
            "Неверный размер скина: {width}x{height} (нужен 64x64, легаси-скины — 64x32)"
        )));
    }
    Ok(())
}

/// Downloads a skin image from an arbitrary URL, size-capped and validated
/// before a single byte is trusted.
pub async fn fetch_skin_bytes(url: &str) -> Result<Vec<u8>> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(NimbusError::Invalid(
            "Ссылка должна начинаться с http:// или https://".to_owned(),
        ));
    }

    let resp = download::client()
        .get(url)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(NimbusError::Http {
            status: status.as_u16(),
            url: url.to_owned(),
            retriable: status.is_server_error(),
        });
    }
    if let Some(len) = resp.content_length() {
        if len > MAX_SKIN_BYTES {
            return Err(NimbusError::Invalid(
                "Файл скина слишком большой (максимум 2 МБ)".to_owned(),
            ));
        }
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;
    if bytes.len() as u64 > MAX_SKIN_BYTES {
        return Err(NimbusError::Invalid(
            "Файл скина слишком большой (максимум 2 МБ)".to_owned(),
        ));
    }
    validate_skin_png(&bytes)?;
    Ok(bytes.to_vec())
}

/// Reads and validates a local file the user picked from disk.
pub async fn read_local_skin(path: &Path) -> Result<Vec<u8>> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|_| NimbusError::Invalid("Файл не найден".to_owned()))?;
    if !meta.is_file() {
        return Err(NimbusError::Invalid(
            "Выбрать нужно файл, а не папку".to_owned(),
        ));
    }
    if meta.len() > MAX_SKIN_BYTES {
        return Err(NimbusError::Invalid(
            "Файл скина слишком большой (максимум 2 МБ)".to_owned(),
        ));
    }
    let bytes = tokio::fs::read(path).await?;
    validate_skin_png(&bytes)?;
    Ok(bytes)
}

/// Saves already-validated skin bytes into the shared skins folder under a
/// stable name for `owner_key` (an offline profile's UUID), overwriting
/// whatever was there before.
pub async fn store_local_skin(owner_key: &str, bytes: &[u8]) -> Result<String> {
    let dir = skins_dir()?;
    tokio::fs::create_dir_all(&dir).await?;
    let safe: String = owner_key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let file_name = format!("{safe}.png");
    tokio::fs::write(dir.join(&file_name), bytes).await?;
    Ok(file_name)
}

/// Best-effort removal: a skin file that is already gone is not an error.
pub fn delete_local_skin(file_name: &str) {
    if let Ok(dir) = skins_dir() {
        let _ = std::fs::remove_file(dir.join(file_name));
    }
}

/// A player's currently-set real skin, as reported by Mojang's public
/// session server (no authentication needed -- this works for any UUID that
/// has a Minecraft profile, not just the signed-in account).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSkin {
    pub url: String,
    pub model: SkinModel,
}

#[derive(Deserialize)]
struct SessionProfileResponse {
    #[serde(default)]
    properties: Vec<SessionProperty>,
}

#[derive(Deserialize)]
struct SessionProperty {
    name: String,
    value: String,
}

#[derive(Deserialize, Default)]
struct TexturesBlock {
    #[serde(rename = "SKIN")]
    skin: Option<TextureEntry>,
}

#[derive(Deserialize)]
struct TexturesPayload {
    #[serde(default)]
    textures: TexturesBlock,
}

#[derive(Deserialize)]
struct TextureEntry {
    url: String,
    #[serde(default)]
    metadata: Option<TextureMetadata>,
}

#[derive(Deserialize)]
struct TextureMetadata {
    #[serde(default)]
    model: Option<String>,
}

/// Looks up a player's current real skin straight from Mojang, so the skin
/// editor can show "here is what you have now" before anything changes.
/// `None` means the profile has no custom skin set (default Steve/Alex) or
/// does not exist.
pub async fn fetch_public_skin(uuid: &str) -> Result<Option<PublicSkin>> {
    let url =
        format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}?unsigned=false");
    let resp = download::client()
        .get(&url)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(NimbusError::Http {
            status: resp.status().as_u16(),
            url,
            retriable: resp.status().is_server_error(),
        });
    }

    let profile: SessionProfileResponse = resp
        .json()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;
    let Some(prop) = profile.properties.iter().find(|p| p.name == "textures") else {
        return Ok(None);
    };

    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&prop.value)
        .map_err(|e| NimbusError::Invalid(format!("Не удалось разобрать ответ Mojang: {e}")))?;
    let payload: TexturesPayload = serde_json::from_slice(&decoded)?;
    let Some(skin) = payload.textures.skin else {
        return Ok(None);
    };
    let model = skin
        .metadata
        .and_then(|m| m.model)
        .map(|m| SkinModel::parse(&m))
        .unwrap_or_default();
    Ok(Some(PublicSkin {
        url: skin.url,
        model,
    }))
}

/// Turns a non-2xx Mojang response into a readable error, using whatever
/// `errorMessage` field the API returned when there is one.
async fn mojang_error(resp: reqwest::Response) -> NimbusError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("errorMessage")
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str())
                .map(str::to_owned)
        })
        .unwrap_or(body);
    NimbusError::Invalid(format!("Mojang отклонил смену скина ({status}): {detail}"))
}

/// Sets the account's real skin from a URL Mojang fetches itself. Shows up
/// for every player, on every server, exactly like the official launcher.
pub async fn upload_skin_url(access_token: &str, url: &str, model: SkinModel) -> Result<()> {
    let body = serde_json::json!({ "variant": model.as_variant(), "url": url });
    let resp = download::client()
        .post(MOJANG_SKIN_API)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(mojang_error(resp).await)
    }
}

/// Sets the account's real skin from local file bytes.
pub async fn upload_skin_file(access_token: &str, bytes: Vec<u8>, model: SkinModel) -> Result<()> {
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name("skin.png")
        .mime_str("image/png")
        .map_err(|e| NimbusError::Invalid(e.to_string()))?;
    let form = reqwest::multipart::Form::new()
        .text("variant", model.as_variant())
        .part("file", part);

    let resp = download::client()
        .post(MOJANG_SKIN_API)
        .bearer_auth(access_token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(mojang_error(resp).await)
    }
}

/// Resets the account back to its default Steve/Alex skin.
pub async fn reset_skin(access_token: &str) -> Result<()> {
    let resp = download::client()
        .delete(format!("{MOJANG_SKIN_API}/active"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(mojang_error(resp).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&0u32.to_be_bytes()); // chunk length (unused by the check)
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    #[test]
    fn accepts_modern_and_legacy_dimensions() {
        assert!(validate_skin_png(&png_header(64, 64)).is_ok());
        assert!(validate_skin_png(&png_header(64, 32)).is_ok());
    }

    #[test]
    fn rejects_wrong_dimensions() {
        assert!(validate_skin_png(&png_header(128, 128)).is_err());
        assert!(validate_skin_png(&png_header(32, 32)).is_err());
    }

    #[test]
    fn rejects_non_png_bytes() {
        assert!(validate_skin_png(b"not a png at all").is_err());
    }

    #[test]
    fn model_variant_round_trips() {
        assert_eq!(SkinModel::parse("slim"), SkinModel::Slim);
        assert_eq!(SkinModel::parse("SLIM"), SkinModel::Slim);
        assert_eq!(SkinModel::parse("classic"), SkinModel::Classic);
        assert_eq!(SkinModel::parse("anything-else"), SkinModel::Classic);
        assert_eq!(SkinModel::Slim.as_variant(), "slim");
        assert_eq!(SkinModel::Classic.as_variant(), "classic");
    }
}
