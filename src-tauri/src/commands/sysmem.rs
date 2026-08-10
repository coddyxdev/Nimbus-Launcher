//! Memory advisor.
//!
//! Players either leave the default 4 GB on a 32 GB machine or hand 24 GB to a
//! vanilla build and wonder why the game stutters. The advice below is the
//! boring, safe middle: enough heap for the mod count, never more than half the
//! machine, and always 2 GB left for the OS.

use serde::Serialize;

use crate::error::Result;
use crate::{config, instance, paths};

use super::shared::validate_instance_id;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAdvice {
    pub system_mib: u64,
    pub available_mib: u64,
    pub mod_count: u32,
    pub recommended_mib: u32,
    /// What the build would use right now, override or global default.
    pub current_mib: u32,
}

/// Enabled `.jar` files only: a `.jar.disabled` mod costs no heap.
fn count_mods(dir: &std::path::Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("jar"))
                .unwrap_or(false)
        })
        .count() as u32
}

/// Heap for the mod count, then clamped to what the machine can spare.
pub fn recommend(system_mib: u64, mod_count: u32) -> u32 {
    let base: u32 = match mod_count {
        0 => 2048,
        1..=39 => 3072,
        40..=99 => 4096,
        100..=199 => 6144,
        _ => 8192,
    };

    // Half the machine is the hard ceiling: the JVM heap is not the only thing
    // the game needs, and Windows plus a browser want the rest.
    let half = u32::try_from(system_mib / 2).unwrap_or(u32::MAX);
    let leave_os = u32::try_from(system_mib.saturating_sub(2048)).unwrap_or(u32::MAX);
    let capped = base.min(half).min(leave_os);

    // Round down to a whole 512 MB step so the number looks deliberate.
    let rounded = (capped / 512) * 512;
    rounded.clamp(1024, 16384)
}

#[tauri::command]
pub async fn recommend_memory(instance_id: String) -> Result<MemoryAdvice> {
    validate_instance_id(&instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let cfg = config::load()?;
    let mods_dir = inst.mods_dir(&instances_dir);

    let (system_mib, available_mib) = tokio::task::spawn_blocking(|| {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        (
            system.total_memory() / (1024 * 1024),
            system.available_memory() / (1024 * 1024),
        )
    })
    .await
    .unwrap_or((0, 0));

    let mod_count = tokio::task::spawn_blocking(move || count_mods(&mods_dir))
        .await
        .unwrap_or(0);

    Ok(MemoryAdvice {
        system_mib,
        available_mib,
        mod_count,
        recommended_mib: recommend(system_mib, mod_count),
        current_mib: inst.memory_mib(cfg.default_memory_mib),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_on_a_big_machine_stays_modest() {
        assert_eq!(recommend(32768, 0), 2048);
    }

    #[test]
    fn heavy_modpack_gets_more_heap() {
        assert_eq!(recommend(32768, 250), 8192);
    }

    #[test]
    fn small_machine_never_gives_away_everything() {
        // 4 GB total: half is 2 GB, and 2 GB must stay for the OS.
        assert_eq!(recommend(4096, 250), 2048);
    }
}
