//! Multiplayer server list.
//!
//! The list is the vanilla `servers.dat` (uncompressed NBT) inside the
//! instance's game directory, not a launcher-private file: whatever the player
//! adds in game shows up here, and whatever is added here shows up in game.
//!
//! Status comes from the standard Server List Ping handshake, so it works with
//! any vanilla-compatible server without an API key or a third-party service.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::{NimbusError, Result};
use crate::{instance, paths};

use super::shared::validate_instance_id;

const DEFAULT_PORT: u16 = 25565;
/// Long enough for a busy server across the ocean, short enough that a dead
/// address does not freeze the list.
const PING_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ip: String,
    /// Base64 PNG the server sent to the client at some point; kept as-is so
    /// rewriting the file never loses an icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(
        default,
        rename = "acceptTextures",
        skip_serializing_if = "Option::is_none"
    )]
    pub accept_textures: Option<i8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ServersFile {
    #[serde(default)]
    servers: Vec<ServerEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub online: bool,
    pub players: u32,
    pub max_players: u32,
    pub version: String,
    pub motd: String,
    pub latency_ms: u64,
    /// Base64 data URL of the server icon, when the server sends one.
    pub favicon: Option<String>,
    /// Why the ping failed; `None` while the server is up.
    pub error: Option<String>,
}

fn offline(reason: String) -> ServerStatus {
    ServerStatus {
        online: false,
        players: 0,
        max_players: 0,
        version: String::new(),
        motd: String::new(),
        latency_ms: 0,
        favicon: None,
        error: Some(reason),
    }
}

/// Splits `host:port`, falling back to the vanilla port.
pub fn split_address(address: &str) -> (String, u16) {
    let trimmed = address.trim();
    match trimmed.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => (
            host.to_owned(),
            port.trim().parse::<u16>().unwrap_or(DEFAULT_PORT),
        ),
        _ => (trimmed.to_owned(), DEFAULT_PORT),
    }
}

fn servers_path(instance_id: &str) -> Result<PathBuf> {
    validate_instance_id(instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, instance_id)?;
    Ok(inst.game_dir(&instances_dir).join("servers.dat"))
}

fn read_list(path: &Path) -> Result<ServersFile> {
    // A build that was never played has no servers.dat; that is an empty list,
    // not an error.
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(ServersFile::default());
    };
    if bytes.is_empty() {
        return Ok(ServersFile::default());
    }
    fastnbt::from_bytes::<ServersFile>(&bytes)
        .map_err(|err| NimbusError::Invalid(format!("servers.dat не читается: {err}")))
}

fn write_list(path: &Path, data: &ServersFile) -> Result<()> {
    let bytes = fastnbt::to_bytes(data)
        .map_err(|err| NimbusError::Invalid(format!("servers.dat не записывается: {err}")))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn joined(handle: std::result::Result<Result<Vec<ServerEntry>>, tokio::task::JoinError>) -> Result<Vec<ServerEntry>> {
    match handle {
        Ok(inner) => inner,
        Err(err) => Err(NimbusError::Invalid(format!(
            "список серверов недоступен: {err}"
        ))),
    }
}

#[tauri::command]
pub async fn list_servers(instance_id: String) -> Result<Vec<ServerEntry>> {
    let path = servers_path(&instance_id)?;
    joined(tokio::task::spawn_blocking(move || read_list(&path).map(|file| file.servers)).await)
}

#[tauri::command]
pub async fn add_server(
    instance_id: String,
    name: String,
    address: String,
) -> Result<Vec<ServerEntry>> {
    let name = name.trim().to_owned();
    let address = address.trim().to_owned();
    if name.is_empty() || address.is_empty() {
        return Err(NimbusError::Invalid(
            "Укажите название и адрес сервера".to_owned(),
        ));
    }
    if name.chars().count() > 64 || address.chars().count() > 128 {
        return Err(NimbusError::Invalid(
            "Слишком длинное название или адрес".to_owned(),
        ));
    }

    let path = servers_path(&instance_id)?;
    joined(
        tokio::task::spawn_blocking(move || {
            let mut file = read_list(&path)?;
            if file.servers.iter().any(|s| s.ip == address) {
                return Err(NimbusError::Invalid(
                    "Такой сервер уже есть в списке".to_owned(),
                ));
            }
            file.servers.push(ServerEntry {
                name,
                ip: address,
                icon: None,
                accept_textures: None,
            });
            write_list(&path, &file)?;
            Ok(file.servers)
        })
        .await,
    )
}

#[tauri::command]
pub async fn remove_server(instance_id: String, address: String) -> Result<Vec<ServerEntry>> {
    let path = servers_path(&instance_id)?;
    joined(
        tokio::task::spawn_blocking(move || {
            let mut file = read_list(&path)?;
            file.servers.retain(|s| s.ip != address);
            write_list(&path, &file)?;
            Ok(file.servers)
        })
        .await,
    )
}

/// Never fails for an unreachable server: "offline" is a normal result the UI
/// shows next to the row, not an error dialog.
#[tauri::command]
pub async fn ping_server(address: String) -> Result<ServerStatus> {
    let (host, port) = split_address(&address);
    if host.is_empty() {
        return Ok(offline("пустой адрес".to_owned()));
    }
    match tokio::time::timeout(PING_TIMEOUT, query(host, port)).await {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(err)) => Ok(offline(err.to_string())),
        Err(_) => Ok(offline("сервер не ответил".to_owned())),
    }
}

async fn query(host: String, port: u16) -> Result<ServerStatus> {
    let started = Instant::now();
    let mut stream = TcpStream::connect((host.as_str(), port)).await?;
    // Nagle batching would add latency to two tiny packets.
    let _ = stream.set_nodelay(true);

    // Handshake. Protocol -1 means "undetermined", which every server still
    // answers in status mode, so this works from 1.7 to the latest release.
    let mut handshake = vec![0x00];
    write_varint(&mut handshake, -1);
    write_string(&mut handshake, &host);
    handshake.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake, 1);
    send_packet(&mut stream, &handshake).await?;

    // Status request: empty body.
    send_packet(&mut stream, &[0x00]).await?;

    let length = read_varint_stream(&mut stream).await?;
    if length <= 0 || length > 4 * 1024 * 1024 {
        return Err(NimbusError::Invalid("некорректный ответ сервера".to_owned()));
    }
    let mut body = vec![0u8; length as usize];
    stream.read_exact(&mut body).await?;
    let latency_ms = started.elapsed().as_millis() as u64;

    let mut cursor: &[u8] = &body;
    let packet_id = read_varint_slice(&mut cursor)?;
    if packet_id != 0x00 {
        return Err(NimbusError::Invalid("неожиданный пакет статуса".to_owned()));
    }
    let json_len = read_varint_slice(&mut cursor)?;
    if json_len < 0 || json_len as usize > cursor.len() {
        return Err(NimbusError::Invalid("обрезанный ответ сервера".to_owned()));
    }
    let json: serde_json::Value = serde_json::from_slice(&cursor[..json_len as usize])
        .map_err(|err| NimbusError::Invalid(format!("ответ сервера не разобран: {err}")))?;

    let players = json.get("players");
    Ok(ServerStatus {
        online: true,
        players: players
            .and_then(|p| p.get("online"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        max_players: players
            .and_then(|p| p.get("max"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        version: json
            .get("version")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
        motd: json
            .get("description")
            .map(flatten_text)
            .map(|text| strip_formatting(&text))
            .unwrap_or_default(),
        latency_ms,
        favicon: json
            .get("favicon")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned()),
        error: None,
    })
}

/// MOTDs arrive as a plain string, a chat component, or a tree of components.
fn flatten_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items.iter().map(flatten_text).collect(),
        serde_json::Value::Object(map) => {
            let mut out = map
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            if let Some(extra) = map.get("extra") {
                out.push_str(&flatten_text(extra));
            }
            out
        }
        _ => String::new(),
    }
}

/// Drops legacy `§`-colour codes so the MOTD renders as plain text.
fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\u{00a7}' {
            chars.next();
            continue;
        }
        out.push(ch);
    }
    out.trim().to_owned()
}

fn write_varint(buf: &mut Vec<u8>, value: i32) {
    let mut rest = value as u32;
    loop {
        let mut byte = (rest & 0x7F) as u8;
        rest >>= 7;
        if rest != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if rest == 0 {
            break;
        }
    }
}

fn write_string(buf: &mut Vec<u8>, value: &str) {
    write_varint(buf, value.len() as i32);
    buf.extend_from_slice(value.as_bytes());
}

async fn send_packet(stream: &mut TcpStream, payload: &[u8]) -> Result<()> {
    let mut framed = Vec::with_capacity(payload.len() + 5);
    write_varint(&mut framed, payload.len() as i32);
    framed.extend_from_slice(payload);
    stream.write_all(&framed).await?;
    Ok(())
}

async fn read_varint_stream(stream: &mut TcpStream) -> Result<i32> {
    let mut result: i32 = 0;
    for shift in 0..5 {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await?;
        result |= ((byte[0] & 0x7F) as i32) << (shift * 7);
        if byte[0] & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(NimbusError::Invalid("слишком длинный VarInt".to_owned()))
}

fn read_varint_slice(data: &mut &[u8]) -> Result<i32> {
    let mut result: i32 = 0;
    for shift in 0..5 {
        let Some((first, rest)) = data.split_first() else {
            return Err(NimbusError::Invalid("обрезанный VarInt".to_owned()));
        };
        *data = rest;
        result |= ((first & 0x7F) as i32) << (shift * 7);
        if first & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(NimbusError::Invalid("слишком длинный VarInt".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_defaults_to_vanilla_port() {
        assert_eq!(split_address("play.example.com"), ("play.example.com".to_owned(), 25565));
        assert_eq!(split_address("play.example.com:25570"), ("play.example.com".to_owned(), 25570));
    }

    #[test]
    fn colour_codes_are_removed() {
        assert_eq!(strip_formatting("\u{00a7}aHello \u{00a7}bworld"), "Hello world");
    }

    #[test]
    fn varint_roundtrip() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 300);
        let mut slice: &[u8] = &buf;
        assert_eq!(read_varint_slice(&mut slice).unwrap(), 300);
    }
}
