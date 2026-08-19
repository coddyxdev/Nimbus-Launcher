//! One-click automatic repair for a broken or incompatible instance.
//!
//! Ties together diagnostics the launcher already has -- the crash analyzer
//! (`files::analyze_text`), the known mod-conflict table, and the mod update
//! checker -- into a single action: take a safety snapshot, then
//! automatically resolve what can be resolved automatically (a stale mod
//! version, a known-bad pair, a commonly-missing base library) and report
//! everything else instead of guessing.
//!
//! Best-effort by nature, exactly like the crash analyzer it builds on: it
//! recognises common failure patterns and can both miss real causes and take
//! an action that does not actually help. The safety snapshot taken at the
//! start is what makes that an acceptable trade -- every change here can be
//! undone from Restore Points.

use serde::Serialize;
use tauri::AppHandle;

use crate::error::Result;
use crate::instance;
use crate::modrinth;
use crate::paths;

use super::files::{self, CrashAnalysis, ModConflict, MOD_CONFLICTS};
use super::mod_updates;
use super::mods::{self, ModInfo};
use super::restore;
use super::shared::{ensure_not_running, validate_instance_id};

/// One step `repair_instance` took, in the order it happened.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairAction {
    /// `"updated"` | `"disabled"` | `"installed"` | `"unresolved"`.
    pub kind: String,
    pub title: String,
    pub detail: String,
}

/// Everything `repair_instance` found and did.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    pub actions: Vec<RepairAction>,
    /// True when a restore point was made before anything else happened.
    pub snapshot_taken: bool,
    /// The crash-analysis findings behind the automatic actions, if a crash
    /// report or a recent log was actually available to analyse.
    pub analysis: Option<CrashAnalysis>,
}

/// Lowercased, alphanumeric-only form of a name, so "Fabric API",
/// "fabric-api" and "FabricAPI.jar" all compare equal.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// True when `installed` already looks like it contains a mod called `title`.
fn already_have(installed: &[ModInfo], title: &str) -> bool {
    let needle = normalize(title);
    if needle.is_empty() {
        return false;
    }
    installed.iter().any(|m| {
        let hay = normalize(&m.file_name);
        hay.contains(&needle) || needle.contains(&hay)
    })
}

/// Finds the on-disk (enabled) mod file whose name contains `needle`.
fn find_enabled(installed: &[ModInfo], needle: &str) -> Option<String> {
    installed
        .iter()
        .find(|m| m.enabled && m.file_name.to_ascii_lowercase().contains(needle))
        .map(|m| m.file_name.clone())
}

/// Reads the most recently modified crash report's text, if any exist.
async fn latest_crash_text(instance_id: &str) -> Option<String> {
    let mut reports = files::list_crash_reports(instance_id.to_owned()).ok()?;
    reports.sort_by_key(|r| std::cmp::Reverse(r.last_modified));
    let newest = reports.into_iter().next()?;
    files::read_crash_report(instance_id.to_owned(), newest.file_name)
        .await
        .ok()
}

/// Reads the tail of the current `latest.log`, capped so a huge log cannot
/// blow up memory or slow the analyzer down. Fabric-style "mod resolution
/// failed" errors often never produce a crash report at all, so this is what
/// catches those.
async fn log_tail(instance_id: &str) -> Option<String> {
    let instances_dir = paths::instances_dir().ok()?;
    let inst = instance::load(&instances_dir, instance_id).ok()?;
    let path = inst.logs_dir(&instances_dir).join("latest.log");
    let bytes = tokio::fs::read(&path).await.ok()?;
    const MAX_BYTES: usize = 256 * 1024;
    let start = bytes.len().saturating_sub(MAX_BYTES);
    Some(String::from_utf8_lossy(&bytes[start..]).into_owned())
}

/// Disables one side of a known-bad mod pair, when both are present.
/// Picks the loader-appropriate side for pairs the static table itself
/// cannot decide (e.g. Sodium vs. Rubidium, the same optimiser split across
/// two loaders).
async fn resolve_conflict(
    instance_id: &str,
    loader: Option<&str>,
    conflict: &ModConflict,
    installed: &[ModInfo],
    actions: &mut Vec<RepairAction>,
) {
    let has = |needle: &str| {
        installed
            .iter()
            .any(|m| m.enabled && m.file_name.to_ascii_lowercase().contains(needle))
    };
    if !(has(conflict.first) && has(conflict.second)) {
        return;
    }

    let target = conflict.auto_disable.or_else(|| {
        if conflict.first == "sodium" && conflict.second == "rubidium" {
            match loader {
                Some("fabric") | Some("quilt") => Some("rubidium"),
                Some("forge") | Some("neoforge") => Some("sodium"),
                _ => None,
            }
        } else {
            None
        }
    });

    let Some(needle) = target else {
        actions.push(RepairAction {
            kind: "unresolved".to_owned(),
            title: conflict.title.to_owned(),
            detail: conflict.suggestion.to_owned(),
        });
        return;
    };

    let Some(file_name) = find_enabled(installed, needle) else {
        return;
    };

    match mods::set_mod_enabled(instance_id.to_owned(), file_name.clone(), false).await {
        Ok(_) => actions.push(RepairAction {
            kind: "disabled".to_owned(),
            title: conflict.title.to_owned(),
            detail: format!("Отключён {file_name}: {}", conflict.suggestion),
        }),
        Err(err) => actions.push(RepairAction {
            kind: "unresolved".to_owned(),
            title: conflict.title.to_owned(),
            detail: format!("Не удалось отключить {file_name}: {err}"),
        }),
    }
}

/// Tries to find and install a mod by display name (e.g. "Fabric API", or a
/// name pulled out of a crash report). Silently skipped when it already
/// looks installed; reported as unresolved when nothing on Modrinth matches.
async fn try_install_missing(
    instance_id: &str,
    loader: Option<&str>,
    mc: Option<&str>,
    title: &str,
    installed: &mut Vec<ModInfo>,
    actions: &mut Vec<RepairAction>,
) {
    let title = title.trim();
    if title.chars().count() < 3 || already_have(installed, title) {
        return;
    }

    let page = match modrinth::search(title, loader, mc, 5, 0, Some("relevance")).await {
        Ok(page) => page,
        Err(_) => return,
    };
    let needle = normalize(title);
    let Some(hit) = page.hits.into_iter().find(|h| {
        let hay = normalize(&h.title);
        hay.contains(&needle) || needle.contains(&hay)
    }) else {
        actions.push(RepairAction {
            kind: "unresolved".to_owned(),
            title: title.to_owned(),
            detail: "Не удалось найти подходящий мод на Modrinth для автоустановки".to_owned(),
        });
        return;
    };

    if already_have(installed, &hit.title) {
        return;
    }

    let version = match modrinth::best_version(&hit.project_id, loader, mc).await {
        Ok(v) => v,
        Err(err) => {
            actions.push(RepairAction {
                kind: "unresolved".to_owned(),
                title: hit.title.clone(),
                detail: format!("Нет подходящей версии под эту сборку: {err}"),
            });
            return;
        }
    };

    let Ok(instances_dir) = paths::instances_dir() else {
        return;
    };
    let Ok(inst) = instance::load(&instances_dir, instance_id) else {
        return;
    };
    let mods_dir = inst.mods_dir(&instances_dir);

    match modrinth::install_version(&mods_dir, &version).await {
        Ok(file_name) => {
            actions.push(RepairAction {
                kind: "installed".to_owned(),
                title: hit.title.clone(),
                detail: format!("Установлен недостающий мод: {file_name}"),
            });
            installed.push(ModInfo {
                file_name,
                size_bytes: 0,
                last_modified: 0,
                enabled: true,
            });
        }
        Err(err) => actions.push(RepairAction {
            kind: "unresolved".to_owned(),
            title: hit.title.clone(),
            detail: format!("Не удалось установить: {err}"),
        }),
    }
}

/// Automatically repairs an instance in one call: snapshots it, updates mods
/// that are outdated for its Minecraft version (the single most common cause
/// of an "incompatible mod set" crash), disables known-bad mod pairs, and
/// installs commonly-missing dependencies the last crash or log points at.
///
/// Every step that cannot be resolved automatically is reported instead of
/// failing the whole repair -- a partial fix plus a clear list of what still
/// needs attention is more useful than an all-or-nothing operation.
#[tauri::command]
pub async fn repair_instance(instance_id: String, app: AppHandle) -> Result<RepairReport> {
    validate_instance_id(&instance_id)?;
    ensure_not_running(&app, &instance_id)?;

    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let loader = inst.loader.clone();
    let mc = inst
        .minecraft_version
        .clone()
        .or_else(|| Some(inst.version_id.clone()));

    let snapshot_taken = restore::auto_snapshot(&instance_id, "Перед автопочинкой")
        .await
        .is_ok();
    let mut report = RepairReport {
        snapshot_taken,
        ..Default::default()
    };

    // 1. Outdated mods are the single most common cause of "incompatible mod
    //    set" crashes: a mod built for an older Minecraft version.
    if let Ok(updates) = mod_updates::check_mod_updates(instance_id.clone()).await {
        for upd in updates {
            match mod_updates::apply_mod_update(
                instance_id.clone(),
                upd.file_name.clone(),
                upd.latest_version_id.clone(),
            )
            .await
            {
                Ok(_) => report.actions.push(RepairAction {
                    kind: "updated".to_owned(),
                    title: upd.title.clone(),
                    detail: format!(
                        "Обновлён с {} до {}",
                        upd.current_version, upd.latest_version
                    ),
                }),
                Err(err) => report.actions.push(RepairAction {
                    kind: "unresolved".to_owned(),
                    title: upd.title,
                    detail: format!("Не удалось обновить: {err}"),
                }),
            }
        }
    }

    let mut installed = mods::list_mods(instance_id.clone()).unwrap_or_default();

    // 2. Known-bad mod pairs: disable whichever side the table (or the
    //    instance's loader) says to drop.
    for conflict in MOD_CONFLICTS {
        resolve_conflict(
            &instance_id,
            loader.as_deref(),
            conflict,
            &installed,
            &mut report.actions,
        )
        .await;
    }
    // Refreshed so the dependency-installation step below sees the mods that
    // just got disabled and does not mistake them for still being active.
    installed = mods::list_mods(instance_id.clone()).unwrap_or(installed);

    // 3. Crash analysis: the most recent crash report plus the log tail,
    //    combined so a Fabric resolution failure (which often never writes a
    //    crash report) is caught the same way a Forge crash is.
    let mut combined = String::new();
    if let Some(text) = latest_crash_text(&instance_id).await {
        combined.push_str(&text);
        combined.push('\n');
    }
    if let Some(text) = log_tail(&instance_id).await {
        combined.push_str(&text);
    }

    if !combined.is_empty() {
        let analysis = files::analyze_text(&combined);

        // "Requires Fabric API"-style findings have one fix: install it.
        if analysis
            .findings
            .iter()
            .any(|f| f.title.contains("Fabric API"))
        {
            try_install_missing(
                &instance_id,
                loader.as_deref(),
                mc.as_deref(),
                "Fabric API",
                &mut installed,
                &mut report.actions,
            )
            .await;
        }

        // Named mods the report or log pointed at, one attempt each, capped
        // so a garbage extraction cannot turn into a search spree.
        for name in analysis.suspected_mods.iter().take(5) {
            try_install_missing(
                &instance_id,
                loader.as_deref(),
                mc.as_deref(),
                name,
                &mut installed,
                &mut report.actions,
            )
            .await;
        }

        report.analysis = Some(analysis);
    }

    Ok(report)
}
