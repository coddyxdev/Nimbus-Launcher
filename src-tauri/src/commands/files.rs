//! Folder shortcuts, crash reports (incl. heuristic analysis), file export and
//! shared-cache cleanup.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{NimbusError, Result};
use crate::instance;
use crate::paths;

use super::shared::{open_external_url, reveal_dir, validate_instance_id};

/// Opens the instance's game directory (`.minecraft`) in Explorer.
#[tauri::command]
pub async fn open_game_dir(instance_id: String) -> Result<()> {
    validate_instance_id(&instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.game_dir(&instances_dir)).await
}

/// Opens the instance's mods directory in Explorer.
#[tauri::command]
pub async fn open_mods_dir(instance_id: String) -> Result<()> {
    validate_instance_id(&instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.mods_dir(&instances_dir)).await
}

/// Opens the instance's screenshots directory in Explorer.
#[tauri::command]
pub async fn open_screenshots_dir(instance_id: String) -> Result<()> {
    validate_instance_id(&instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.game_dir(&instances_dir).join("screenshots")).await
}

/// Opens the instance's crash-reports directory in Explorer.
#[tauri::command]
pub async fn open_crash_reports_dir(instance_id: String) -> Result<()> {
    validate_instance_id(&instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.game_dir(&instances_dir).join("crash-reports")).await
}

/// Opens the launcher's own log directory for an instance.
#[tauri::command]
pub async fn open_logs_dir(instance_id: String) -> Result<()> {
    validate_instance_id(&instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.logs_dir(&instances_dir)).await
}

/// Opens the launcher's own log folder (`launcher.log`), which is what bug
/// reports need and what nobody can find on their own.
#[tauri::command]
pub async fn open_launcher_logs_dir() -> Result<()> {
    reveal_dir(&paths::logs_dir()?).await
}

/// Opens an external link (mod description, changelog) in the system browser.
#[tauri::command]
pub fn open_url(url: String) -> Result<()> {
    open_external_url(&url)
}

/// Metadata for a crash report file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReportInfo {
    pub(crate) file_name: String,
    size_bytes: u64,
    pub(crate) last_modified: u64,
}

#[tauri::command]
pub fn list_crash_reports(instance_id: String) -> Result<Vec<CrashReportInfo>> {
    validate_instance_id(&instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let dir = inst.game_dir(&instances_dir).join("crash-reports");

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut reports: Vec<CrashReportInfo> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .map(|e| e == "txt" || e == "log")
            .unwrap_or(false)
        {
            if let Ok(metadata) = std::fs::metadata(&path) {
                reports.push(CrashReportInfo {
                    file_name: entry.file_name().to_string_lossy().to_string(),
                    size_bytes: metadata.len(),
                    last_modified: metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                });
            }
        }
    }
    reports.sort_by_key(|r| std::cmp::Reverse(r.last_modified));
    Ok(reports)
}

/// Reads one crash report so the UI can show it without opening Explorer.
///
/// The file name is validated, so this cannot be used to read arbitrary paths.
#[tauri::command]
pub async fn read_crash_report(instance_id: String, file_name: String) -> Result<String> {
    use super::shared::validate_file_name;

    validate_instance_id(&instance_id)?;
    validate_file_name(&file_name)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let path = inst
        .game_dir(&instances_dir)
        .join("crash-reports")
        .join(&file_name);

    if !path.is_file() {
        return Err(NimbusError::Invalid("Краш-репорт не найден".to_owned()));
    }

    // Crash reports are a few dozen KB; a cap keeps a pathological file from
    // freezing the WebView.
    const MAX_BYTES: u64 = 4 * 1024 * 1024;
    let size = tokio::fs::metadata(&path).await?.len();
    if size > MAX_BYTES {
        return Err(NimbusError::Invalid(
            "Краш-репорт слишком большой — откройте его в папке".to_owned(),
        ));
    }

    let bytes = tokio::fs::read(&path).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// One diagnosis produced by the crash analyzer for a single matched pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashFinding {
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) suggestion: String,
}

/// Result of analyzing one crash report's text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashAnalysis {
    pub(crate) findings: Vec<CrashFinding>,
    /// Mod names pulled from "-- Head --"/"-- Affected level --"-style crash
    /// markers, when the report names a specific mod as involved.
    pub(crate) suspected_mods: Vec<String>,
}

/// One heuristic rule: if `needle` (case-insensitive) appears in the crash
/// text, its finding is reported. Order matters only for display order in a
/// single report -- rules are independent and non-exclusive.
struct CrashRule {
    needle: &'static str,
    title: &'static str,
    detail: &'static str,
    suggestion: &'static str,
}

const CRASH_RULES: &[CrashRule] = &[
    CrashRule {
        needle: "incompatible mod set",
        title: "Несовместимый набор модов",
        detail: "Загрузчик отказался стартовать: версии модов не сочетаются друг с другом или с версией игры.",
        suggestion: "Откатитесь к последней точке восстановления или обновите моды, которые отмечены в отчёте как конфликтующие.",
    },
    CrashRule {
        needle: "requires fabric api",
        title: "Не установлен Fabric API",
        detail: "Моду нужен Fabric API — базовая библиотека, которую большинство Fabric-модов требуют отдельно.",
        suggestion: "Установите Fabric API из каталога модов — версию ровно под вашу версию Minecraft.",
    },
    CrashRule {
        needle: "net.minecraftforge.fml.common.missingmods",
        title: "Forge не нашёл нужные моды",
        detail: "Один или несколько модов требуют другие моды, которых нет в сборке.",
        suggestion: "Установите мод через каталог с включённой установкой зависимостей — они подтянутся автоматически.",
    },
    CrashRule {
        needle: "optifine",
        title: "В сборке есть OptiFine",
        detail: "OptiFine часто конфликтует с модами-оптимизаторами (Sodium, Embeddium, Rubidium) и с частью Fabric-модов.",
        suggestion: "Оставьте что-то одно: либо OptiFine, либо Sodium/Embeddium вместе с Iris для шейдеров.",
    },
    CrashRule {
        needle: "pixel format not accelerated",
        title: "Видеодрайвер не поддерживает ускорение",
        detail: "Система не смогла создать аппаратно ускоренный OpenGL-контекст — обычно это старые или стандартные драйвера Windows.",
        suggestion: "Установите драйвер с сайта NVIDIA / AMD / Intel и убедитесь, что игра запускается на дискретной видеокарте.",
    },
    CrashRule {
        needle: "no space left on device",
        title: "На диске нет места",
        detail: "Запись файла не удалась — закончилось свободное место на диске.",
        suggestion: "Освободите место или воспользуйтесь автоочисткой в «Обслуживании».",
    },
    CrashRule {
        needle: "access is denied",
        title: "Нет прав на файлы сборки",
        detail: "Система запретила доступ к файлу — обычно это антивирус, папка в Program Files или файл, занятый другим процессом.",
        suggestion: "Добавьте папку лаунчера в исключения антивируса и держите сборки вне Program Files.",
    },
    CrashRule {
        needle: "could not reserve enough space",
        title: "Java не смогла выделить память",
        detail: "Запрошено больше памяти, чем система готова дать (часто — 32-битная Java или слишком большой -Xmx).",
        suggestion: "Уменьшите выделенную память в настройках и убедитесь, что используется 64-битная Java.",
    },
    CrashRule {
        needle: "unsupportedclassversionerror",
        title: "Слишком старая Java",
        detail: "Мод или версия игры требуют более новую Java, чем та, которой был выполнен запуск.",
        suggestion: "Включите автоподбор Java в настройках — лаунчер скачает нужную версию сам.",
    },
    CrashRule {
        needle: "connection refused",
        title: "Нет связи с сервером",
        detail: "Игра не смогла подключиться к серверу или к сетевому сервису.",
        suggestion: "Проверьте интернет, адрес сервера и настройки брандмауэра для javaw.exe.",
    },
    CrashRule {
        needle: "invalid session",
        title: "Сессия аккаунта устарела",
        detail: "Сервер отклонил вход: токен авторизации больше не действует.",
        suggestion: "Перезайдите в аккаунт Microsoft в настройках лаунчера.",
    },
    CrashRule {
        needle: "java.lang.outofmemoryerror",
        title: "Нехватка памяти (OutOfMemoryError)",
        detail: "Игре не хватило выделенной оперативной памяти Java.",
        suggestion: "Увеличьте выделенную память в настройках экземпляра или уменьшите количество модов/пакетов ресурсов.",
    },
    CrashRule {
        needle: "mixin apply failed",
        title: "Конфликт Mixin-модов",
        detail: "Один из модов не смог применить свои изменения к коду игры или другому моду (Mixin).",
        suggestion: "Проверьте совместимость версий модов друг с другом и с версией загрузчика; обновите или удалите конфликтующий мод.",
    },
    CrashRule {
        needle: "duplicate mod",
        title: "Дублирующийся мод",
        detail: "В папке mods обнаружено несколько версий одного и того же мода.",
        suggestion: "Откройте папку модов и удалите старые/лишние копии .jar-файлов.",
    },
    CrashRule {
        needle: "which is missing!",
        title: "Не хватает зависимости",
        detail: "Одному из модов требуется другой мод (библиотека), которого нет в игре.",
        suggestion: "Проверьте страницу мода на Modrinth/CurseForge — обычно там указаны обязательные зависимости, и установите их.",
    },
    CrashRule {
        needle: "missing or unsupported mandatory dependencies",
        title: "Не хватает обязательной зависимости",
        detail: "Forge/NeoForge не смог загрузить один из модов, потому что не найдена обязательная зависимость нужной версии.",
        suggestion: "Установите указанный в отчёте мод-зависимость нужной версии или обновите мод, которому она требуется.",
    },
    CrashRule {
        // Narrowed from a bare "opengl": every normal launch logs lines such
        // as "OpenGL Version", so that needle matched almost every crash
        // report and pushed the real diagnosis out of the list.
        needle: "opengl error",
        title: "Проблема с видеокартой/OpenGL",
        detail: "Сбой связан с видеодрайвером или отсутствием поддержки нужной версии OpenGL.",
        suggestion: "Обновите драйвера видеокарты до последней версии и перезапустите компьютер.",
    },
    CrashRule {
        needle: "unsupported class file major version",
        title: "Неподходящая версия Java",
        detail: "Игра или мод собраны для другой версии Java, чем та, что запущена сейчас.",
        suggestion: "В настройках экземпляра проверьте выбранный Java или сбросьте его на автоопределение.",
    },
    CrashRule {
        needle: "watchdog",
        title: "Завис сервера/мира (Watchdog)",
        detail: "Основной поток игры завис и не отвечал слишком долго — лаунчер ватчдога принудительно завершил процесс.",
        suggestion: "Часто вызвано тяжёлым модом или бесконечным циклом в моде/командном блоке; попробуйте отключить недавно добавленные моды.",
    },
    CrashRule {
        // Narrowed from a bare "chunk": chunk loading is mentioned in ordinary
        // log output as well, so this rule used to fire on crashes that had
        // nothing to do with a damaged world.
        needle: "failed to save chunk",
        title: "Возможное повреждение чанков/мира",
        detail: "Сбой упоминает чанки мира — возможно, часть сохранения повреждена.",
        suggestion: "Сделайте резервную копию мира перед дальнейшими действиями и попробуйте инструменты восстановления чанков (например, удалить повреждённый регион).",
    },
    CrashRule {
        needle: "eXCEPTION_ACCESS_VIOLATION",
        title: "Аварийное завершение JVM (native crash)",
        detail: "Процесс аварийно завершился на уровне нативного кода (видеодрайвер, нативные библиотеки модов).",
        suggestion: "Обновите драйвера видеокарты и проверьте, не конфликтуют ли друг с другом моды с нативными библиотеками (например, шейдеры или оптимизаторы текстур).",
    },
    CrashRule {
        needle: "failed to check session lock",
        title: "Мир уже открыт в другом процессе",
        detail: "Игра не смогла получить блокировку мира, потому что он уже открыт другим экземпляром игры.",
        suggestion: "Закройте другие запущенные экземпляры этого же мира и попробуйте снова.",
    },
    CrashRule {
        needle: "mixin apply",
        title: "Ошибка применения Mixin",
        detail: "Мод пытался изменить класс игры через Mixin и не смог — обычно это значит, что версия мода не подходит к версии игры или два мода правят одно и то же место.",
        suggestion: "Найдите в отчёте имя мода рядом с mixin и обновите или отключите именно его.",
    },
    CrashRule {
        needle: "duplicatemodsfound",
        title: "Один и тот же мод установлен дважды",
        detail: "В папке mods лежат две копии одного мода, часто разных версий.",
        suggestion: "Откройте вкладку «Моды» и удалите старую копию — должен остаться только один файл мода.",
    },
    CrashRule {
        needle: "pixel format not accelerated",
        title: "Видеодрайвер не даёт ускорение",
        detail: "OpenGL не смог создать ускоренный контекст: чаще всего виноват старый или сломанный драйвер видеокарты.",
        suggestion: "Обновите драйвер видеокарты с сайта NVIDIA, AMD или Intel и убедитесь, что игра запускается на дискретной видеокарте.",
    },
    CrashRule {
        needle: "unsupportedclassversionerror",
        title: "Мод собран под более новую Java",
        detail: "Классы мода скомпилированы для версии Java выше той, что запускает игру.",
        suggestion: "Сбросьте путь к Java в настройках на автоопределение — лаунчер сам скачает подходящую версию.",
    },
    CrashRule {
        needle: "missing or unsupported mandatory dependencies",
        title: "Не хватает зависимостей модов",
        detail: "Загрузчик перечислил моды, которым нужны другие моды или их другие версии.",
        suggestion: "Установите перечисленные в отчёте моды через каталог — версии подберутся автоматически.",
    },
];

/// Pairs of mods that are known to break each other. Both names appearing in
/// one report is a much stronger signal than either name alone, and the fix is
/// always "keep one of the two", which no generic rule above can say.
///
/// Needles are lowercase: the analyzer matches against lowercased crash text.
pub(crate) struct ModConflict {
    pub(crate) first: &'static str,
    pub(crate) second: &'static str,
    pub(crate) title: &'static str,
    pub(crate) detail: &'static str,
    pub(crate) suggestion: &'static str,
    /// Which of the two the automatic repairer should disable when both are
    /// present. `None` when the right side depends on the instance's loader
    /// (the repairer resolves those itself) or when disabling one
    /// automatically would be a worse guess than asking the user.
    pub(crate) auto_disable: Option<&'static str>,
}

pub(crate) const MOD_CONFLICTS: &[ModConflict] = &[
    ModConflict {
        first: "optifine",
        second: "sodium",
        title: "OptiFine и Sodium вместе",
        detail: "Оба мода переписывают рендер игры и не уживаются в одной сборке.",
        suggestion: "Оставьте что-то одно: Sodium с Iris для шейдеров или OptiFine без Sodium.",
        auto_disable: Some("optifine"),
    },
    ModConflict {
        first: "optifine",
        second: "rubidium",
        title: "OptiFine и Rubidium вместе",
        detail: "Rubidium — это порт Sodium для Forge, и он конфликтует с OptiFine так же.",
        suggestion: "Удалите OptiFine или Rubidium — вместе они не работают.",
        auto_disable: Some("optifine"),
    },
    ModConflict {
        first: "sodium",
        second: "rubidium",
        title: "Sodium и Rubidium вместе",
        detail: "Это две версии одного и того же оптимизатора для разных загрузчиков.",
        suggestion: "Оставьте тот, что соответствует загрузчику сборки: Sodium для Fabric, Rubidium для Forge.",
        // Loader-dependent: the automatic repairer decides at runtime.
        auto_disable: None,
    },
    ModConflict {
        first: "iris",
        second: "optifine",
        title: "Iris и OptiFine вместе",
        detail: "Iris и OptiFine — два разных движка шейдеров, одновременно они не работают.",
        suggestion: "Для шейдеров оставьте связку Sodium + Iris и удалите OptiFine.",
        auto_disable: Some("optifine"),
    },
    ModConflict {
        first: "phosphor",
        second: "starlight",
        title: "Phosphor и Starlight вместе",
        detail: "Оба мода заменяют движок освещения и конфликтуют.",
        suggestion: "Оставьте только Starlight — он быстрее и активнее поддерживается.",
        auto_disable: Some("phosphor"),
    },
    ModConflict {
        first: "feature_nbt_deadlock",
        second: "lithium",
        title: "Lithium и старые патчи мирогенерации",
        detail: "Lithium меняет генерацию мира и известно ломается в паре со старыми модами-патчами.",
        suggestion: "Обновите Lithium до версии под вашу версию игры или отключите его для проверки.",
        // Disabling Lithium to "fix" this would remove the very optimisation
        // most users installed the mod for -- surface it instead of guessing.
        auto_disable: None,
    },
];

/// Pulls mod names out of Forge/NeoForge-style "-- Head --"/"Affected level"
/// crash report sections such as:
/// ```text
/// -- MOD example_mod --
/// Details:
/// ```
/// Heuristic and best-effort: it exists to give the user a starting point,
/// not an authoritative diagnosis.
fn extract_suspected_mods(text: &str) -> Vec<String> {
    let mut mods = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("-- ")
            .and_then(|s| s.strip_suffix(" --"))
        {
            if let Some(name) = rest.strip_prefix("MOD ") {
                let name = name.trim().to_owned();
                if !name.is_empty() && !mods.contains(&name) {
                    mods.push(name);
                }
            }
        }
        // "Caused by: ... (mod_id)" style hints used by some crash handlers.
        if let Some(idx) = trimmed.find("mod '") {
            if let Some(rest) = trimmed.get(idx + 5..) {
                if let Some(end) = rest.find('\'') {
                    let name = rest[..end].trim().to_owned();
                    if !name.is_empty() && !mods.contains(&name) {
                        mods.push(name);
                    }
                }
            }
        }
    }
    mods
}

/// Matches crash text against the known rule table and pulls out any
/// suspected mod names. Pure and synchronous so it is directly unit-testable
/// without touching the filesystem.
pub(crate) fn analyze_text(text: &str) -> CrashAnalysis {
    let lower = text.to_ascii_lowercase();
    let mut findings: Vec<CrashFinding> = CRASH_RULES
        .iter()
        .filter(|rule| lower.contains(&rule.needle.to_ascii_lowercase()))
        .map(|rule| CrashFinding {
            title: rule.title.to_owned(),
            detail: rule.detail.to_owned(),
            suggestion: rule.suggestion.to_owned(),
        })
        .collect();

    // Known-bad pairs come last: they are the most actionable advice, and the
    // UI renders findings in order, so they sit right above the raw report.
    for conflict in MOD_CONFLICTS {
        if lower.contains(conflict.first) && lower.contains(conflict.second) {
            findings.push(CrashFinding {
                title: conflict.title.to_owned(),
                detail: conflict.detail.to_owned(),
                suggestion: conflict.suggestion.to_owned(),
            });
        }
    }

    CrashAnalysis {
        findings,
        suspected_mods: extract_suspected_mods(text),
    }
}

/// Same conflict table, applied to the file names in an instance's mods folder
/// so the user can find a bad pair before the game crashes.
#[tauri::command]
pub async fn check_mod_conflicts(instance_id: String) -> Result<Vec<CrashFinding>> {
    validate_instance_id(&instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let mods_dir = inst.mods_dir(&instances_dir);

    let names = tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&mods_dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                // Disabled mods are not loaded, so they cannot conflict.
                name.ends_with(".jar").then_some(name)
            })
            .collect::<Vec<String>>()
    })
    .await
    .unwrap_or_default();

    let has = |needle: &str| names.iter().any(|name| name.contains(needle));

    Ok(MOD_CONFLICTS
        .iter()
        .filter(|conflict| has(conflict.first) && has(conflict.second))
        .map(|conflict| CrashFinding {
            title: conflict.title.to_owned(),
            detail: conflict.detail.to_owned(),
            suggestion: conflict.suggestion.to_owned(),
        })
        .collect())
}

/// Reads a crash report and runs the heuristic analyzer over it in one call,
/// so the UI does not need to fetch the raw text first.
///
/// This is a best-effort tool, not a substitute for reading the actual
/// report: it only recognises a fixed set of common failure patterns and can
/// both miss real causes and flag unrelated text that happens to match.
#[tauri::command]
pub async fn analyze_crash_report(instance_id: String, file_name: String) -> Result<CrashAnalysis> {
    let text = read_crash_report(instance_id, file_name).await?;
    Ok(analyze_text(&text))
}

/// Extensions the launcher is willing to write through [`save_text_file`].
const EXPORTABLE_EXTENSIONS: [&str; 5] = ["log", "txt", "json", "csv", "md"];

/// Validates a user-chosen export destination and returns the path to write.
///
/// The path comes from the frontend, so it is treated as untrusted: only plain
/// text extensions are allowed and system locations are refused, so the command
/// cannot be turned into a general "write anywhere" primitive. Kept pure (the
/// system directories are passed in) so every rule is unit-testable without
/// touching the filesystem or the environment.
fn check_export_path(path: &str, system_dirs: &[std::path::PathBuf]) -> Result<std::path::PathBuf> {
    let target = Path::new(path);
    if !target.is_absolute() {
        return Err(NimbusError::Invalid(
            "ожидался абсолютный путь для сохранения".to_owned(),
        ));
    }
    // UNC and verbatim prefixes bypass the normalisation below.
    if path.starts_with(r"\\") {
        return Err(NimbusError::Invalid(
            "сетевые пути не поддерживаются".to_owned(),
        ));
    }
    if target.components().any(|c| c.as_os_str() == "..") {
        return Err(NimbusError::Invalid("недопустимый путь".to_owned()));
    }

    let allowed_ext = target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .map(|e| EXPORTABLE_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or(false);
    if !allowed_ext {
        return Err(NimbusError::Invalid(format!(
            "сохранять можно только текстовые файлы ({})",
            EXPORTABLE_EXTENSIONS.join(", ")
        )));
    }

    // Never write into Windows or the installed program directories. Compared
    // component-by-component (not a raw string prefix) so "C:\Program Files
    // 2\..." is not mistaken for being under "C:\Program Files\...", and other
    // paths that merely share a text prefix are neither blocked nor let
    // through by accident.
    let target_lower: Vec<String> = target
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    let is_forbidden = system_dirs.iter().any(|dir| {
        let dir_lower: Vec<String> = dir
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect();
        !dir_lower.is_empty()
            && target_lower.len() >= dir_lower.len()
            && target_lower[..dir_lower.len()] == dir_lower[..]
    });
    if is_forbidden {
        return Err(NimbusError::Invalid(
            "запись в системные папки запрещена".to_owned(),
        ));
    }

    Ok(target.to_path_buf())
}

/// Directories the export command must never write into.
fn forbidden_system_dirs() -> Vec<std::path::PathBuf> {
    ["WINDIR", "ProgramFiles", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .collect()
}

/// Writes UTF-8 text to an absolute path chosen by the user in a save dialog.
/// Used by the console "export log" action. See [`check_export_path`] for the
/// rules the destination has to satisfy.
#[tauri::command]
pub async fn save_text_file(path: String, contents: String) -> Result<()> {
    let target = check_export_path(&path, &forbidden_system_dirs())?;

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&target, contents.as_bytes()).await?;
    Ok(())
}

/// Result of a shared-cache cleanup pass.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    removed_files: u64,
    freed_bytes: u64,
}

/// Removes cached installer jars, their `.processed-*` markers and directories
/// whose names contain `:` (left behind by older Forge installer runs on
/// Windows, where such names are unusable).
#[tauri::command]
pub async fn cleanup_shared() -> Result<CleanupReport> {
    let shared_dir = paths::shared_dir()?;

    tokio::task::spawn_blocking(move || {
        let mut removed_files = 0u64;
        let mut freed_bytes = 0u64;

        let installers = shared_dir.join("installers");
        if let Ok(entries) = std::fs::read_dir(&installers) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let is_installer = name.ends_with(".jar");
                let is_marker = name.starts_with(".processed-");
                if !(is_installer || is_marker) {
                    continue;
                }
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if std::fs::remove_file(&path).is_ok() {
                    removed_files += 1;
                    freed_bytes += size;
                }
            }
        }

        // Junk directories with a colon in the name, e.g.
        // `libraries/net/minecraft/client/1.21:mappings@tsrg`.
        fn sweep_colons(dir: &Path, removed: &mut u64, freed: &mut u64, depth: u32) {
            if depth == 0 {
                return;
            }
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if !file_type.is_dir() {
                    continue;
                }
                let path = entry.path();
                if entry.file_name().to_string_lossy().contains(':') {
                    let size = instance::dir_size(&path);
                    if std::fs::remove_dir_all(&path).is_ok() {
                        *removed += 1;
                        *freed += size;
                    }
                } else {
                    sweep_colons(&path, removed, freed, depth - 1);
                }
            }
        }
        sweep_colons(
            &shared_dir.join("libraries"),
            &mut removed_files,
            &mut freed_bytes,
            12,
        );

        CleanupReport {
            removed_files,
            freed_bytes,
        }
    })
    .await
    .map_err(|e| NimbusError::Invalid(format!("cleanup task failed: {e}")))
}

#[cfg(test)]
mod export_path_tests {
    use super::*;

    fn none() -> Vec<std::path::PathBuf> {
        Vec::new()
    }

    #[test]
    fn accepts_plain_text_destinations() {
        assert!(check_export_path(r"C:\Users\me\Desktop\game.log", &none()).is_ok());
        assert!(check_export_path(r"C:\Users\me\report.TXT", &none()).is_ok());
    }

    #[test]
    fn rejects_relative_paths_and_traversal() {
        assert!(check_export_path(r"logs\game.log", &none()).is_err());
        assert!(check_export_path(r"C:\Users\me\..\other\game.log", &none()).is_err());
    }

    #[test]
    fn rejects_network_paths() {
        assert!(check_export_path(r"\\server\share\game.log", &none()).is_err());
    }

    #[test]
    fn rejects_non_text_extensions() {
        assert!(check_export_path(r"C:\Users\me\evil.exe", &none()).is_err());
        assert!(check_export_path(r"C:\Users\me\evil.bat", &none()).is_err());
        assert!(check_export_path(r"C:\Users\me\noextension", &none()).is_err());
    }

    #[test]
    fn rejects_system_dirs_without_blocking_similar_names() {
        let dirs = vec![std::path::PathBuf::from(r"C:\Program Files")];
        assert!(check_export_path(r"C:\Program Files\Nimbus\log.txt", &dirs).is_err());
        assert!(check_export_path(r"C:\Program Files 2\Nimbus\log.txt", &dirs).is_ok());
    }
}

#[cfg(test)]
mod crash_analyzer_tests {
    use super::*;

    #[test]
    fn detects_out_of_memory() {
        let text = "Exception in thread \"main\" java.lang.OutOfMemoryError: Java heap space";
        let analysis = analyze_text(text);
        assert!(analysis.findings.iter().any(|f| f.title.contains("памяти")));
    }

    #[test]
    fn detects_mixin_conflict_and_extracts_mod_name() {
        let text = "-- MOD example_mod --\nDetails:\nMixin apply failed cannot resolve target";
        let analysis = analyze_text(text);
        assert!(analysis.findings.iter().any(|f| f.title.contains("Mixin")));
        assert_eq!(analysis.suspected_mods, vec!["example_mod".to_owned()]);
    }

    #[test]
    fn clean_text_produces_no_findings() {
        let analysis = analyze_text("Game exited normally, nothing to see here.");
        assert!(analysis.findings.is_empty());
        assert!(analysis.suspected_mods.is_empty());
    }

    #[test]
    fn detects_multiple_independent_patterns() {
        let text = "java.lang.OutOfMemoryError\n...\nwatchdog thread detected a hang";
        let analysis = analyze_text(text);
        assert!(analysis.findings.len() >= 2);
    }
}
