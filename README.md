# Nimbus Client

Минималистичный лаунчер для Minecraft: Rust + Tauri 2 в бэкенде,
TypeScript + Svelte 5 + Vite во фронтенде, CSS без фреймворков.

## Требования

- Windows 10/11 x64
- Node.js 20+
- Rust stable MSVC: `winget install Rustlang.Rustup`, затем
  `rustup default stable-x86_64-pc-windows-msvc`
- Build Tools for Visual Studio с workload «Desktop development with C++» и Windows SDK
- WebView2 (в Windows 11 уже есть; NSIS-установщик дотягивает bootstrapper)

## Сборка

```bash
npm install

# разработка
npm run tauri dev

# release: портативный .exe + NSIS installer
npm run tauri build
```

Результаты:

- `src-tauri/target/release/nimbus-client.exe` — портативный бинарник
- `src-tauri/target/release/bundle/nsis/*.exe` — установщик

### Не собирать через `cargo build --release`

`cargo build --release` компилирует только Rust и не запускает `vite build`.
Фронтенд не попадает в бинарник, WebView откатывается на `devUrl` и ты
видишь `ERR_CONNECTION_REFUSED` вместо интерфейса. В `tauri.conf.json` задан
`"frontendDist": "../dist"`, и `build.outDir` в `vite.config.ts` с ним совпадает —
не рассинхронизируй их.

## Проверки

```bash
cd src-tauri
cargo clippy --all-targets -- -D warnings
cargo test
cd ..
npm run build   # svelte-check + vite build
```

## Структура

```
nimbus-client/
├─ index.html
├─ package.json
├─ svelte.config.js
├─ tsconfig.json
├─ vite.config.ts
├─ docs/
│  └─ IPC.md                  IPC-контракт этапа 1 и согласованный на этап 2
├─ src/
│  ├─ main.ts
│  ├─ app.css                 базовые стили и примитивы
│  ├─ styles/tokens.css       все токены: цвет, типографика, сетка, мотион
│  ├─ lib/
│  │  ├─ ipc.ts               типизированный слой invoke + нормализация ошибок
│  │  ├─ theme.ts             единственное место, где пишется data-theme
│  │  └─ icons.ts             один набор, штрих 1.5, сетка 24
│  └─ components/
│     ├─ App.svelte           оболочка и машина состояний
│     ├─ Titlebar.svelte      drag-region, snap, свои кнопки окна
│     ├─ Rail.svelte          вертикальный рейл сборок + hover-подпись
│     ├─ Header.svelte        контекстный хедер
│     ├─ EmptyState.svelte    пустые и ошибочные состояния
│     ├─ Skeleton.svelte      загрузка без спиннеров
│     ├─ Onboarding.svelte    ровно два экрана
│     └─ Icon.svelte
└─ src-tauri/
   ├─ Cargo.toml              opt-level=z, LTO, strip; panic остаётся unwind
   ├─ build.rs
   ├─ tauri.conf.json
   ├─ capabilities/default.json
   ├─ icons/
   └─ src/
      ├─ main.rs
      ├─ lib.rs               IPC-команды этапа 1
      ├─ config.rs            атомарная запись, миграции, тесты
      ├─ paths.rs             %APPDATA%\NimbusClient и подкаталоги
      └─ error.rs             один тип ошибки, сериализуется как { kind, message }
```

## Дизайн-решения, зафиксированные в токенах

- Фоны: `#101113` → `#16181b` → `#1c1f23` → `#23272c`. Холодные нейтральные серые,
  ни чистого чёрного, ни чистого белого.
- Акцент ровно один: `#5b9dd9`. Используется только для активного состояния,
  фокуса, прогресса и кнопки ИГРАТЬ.
- Типографика: одна гарнитура, размеры 11/12/13/15/20, иерархия через вес и цвет.
  Цифры прогресса и размеров файлов — табличные (`.tnum`).
- Сетка 4 px, радиусы 3–10 px, тень только у всплывающих слоёв.
- Анимации 120/160/200 ms, `cubic-bezier(0.16, 0.85, 0.3, 1)`. При
  `prefers-reduced-motion` все длительности сводятся к 1 ms.
- Активный элемент рейла показан полоской 2 px на кромке, а не залитой
  пилюлей.
- Пустые состояния выровнены по левому краю со смещением — центрированный
  hero-блок запрещён.
- Нет эмодзи, градиентов, блюра и свечений ни в одном файле.
