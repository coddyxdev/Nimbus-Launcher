# IPC контракт Nimbus Client

Все ошибки пересекают границу в виде `{ kind: string, message: string }`.
Никакие команды не возвращают фиктивные данные: если функции ещё нет,
команда отсутствует, а UI явно помечает раздел как недоступный.

## Этап 1 — реализовано

| Команда | Вход | Выход | Назначение |
| --- | --- | --- | --- |
| `bootstrap` | — | `Bootstrap` | Конфиг, версия, папка данных, доступность OAuth — одним вызовом при первой отрисовке |
| `set_theme` | `theme: "dark" \| "light" \| "system"` | `Config` | Смена темы с атомарной записью |
| `set_offline_username` | `username: string` | `Config` | Валидация 1–16 символов, `[A-Za-z0-9_]` |
| `complete_onboarding` | — | `Config` | Закрытие двухэкранного онбординга |
| `list_instances` | — | `Instance[]` | На этапе 1 всегда пустой массив, чтобы UI рисовал реальное пустое состояние |

Управление окном (minimize / toggleMaximize / close / drag) идёт через штатный
`@tauri-apps/api/window`, а не через свои команды. Нужные разрешения перечислены
в `src-tauri/capabilities/default.json`.

## Этап 2 — согласованный контракт (кода ещё нет)

| Команда | Вход | Выход |
| --- | --- | --- |
| `list_versions` | `{ includeSnapshots: bool }` | `VersionSummary[]` |
| `install_version` | `{ versionId: string, instanceId: string }` | События `install:progress` |
| `launch_instance` | `{ instanceId: string }` | `{ pid: number }` |
| `kill_instance` | `{ instanceId: string }` | — |
| `resolve_java` | `{ majorVersion: number }` | `{ path: string, downloaded: bool }` |

События, которые будет слушать фронтенд:

- `install:progress` — `{ stage, file, done, total, bytesDone, bytesTotal }`
- `game:log` — `{ instanceId, level, line }` (этап 4, построчно)
- `game:exit` — `{ instanceId, code }`
