<script lang="ts">
	import { untrack } from "svelte"
	import Icon from "./Icon.svelte"
	import {
		ipc,
		type CleanupReport,
		type Config,
		type ConfigUpdate,
		type JavaInfo,
		type NimbusError,
		type StorageUsage,
	} from "$lib/ipc"
	import { open as openDialog } from "@tauri-apps/plugin-dialog"
	import { sound } from "$lib/sound.svelte"
	import { toasts } from "$lib/toast.svelte"
	import { i18n, LANGS, t, tf } from "$lib/i18n.svelte"
	import { updater } from "$lib/updater.svelte"

	let {
		config,
		devMode = $bindable(false),
		onconfig,
	}: {
		config: Config
		devMode?: boolean
		onconfig: (next: Config) => void
	} = $props()

	// Local draft, seeded once from the saved config when the pane mounts.
	// `untrack` makes it explicit that later config changes must not clobber
	// what the user is currently typing.
	const initial = untrack(() => ({
		memory: config.defaultMemoryMib,
		aikar: config.defaultAikarFlags,
		jvmArgs: config.defaultJvmArgs.join("\n"),
		javaPath: config.javaPath ?? "",
		width: config.gameWidth ?? 0,
		height: config.gameHeight ?? 0,
		fullscreen: config.gameFullscreen,
		discord: config.discordRpc,
	}))

	let memory = $state(initial.memory)
	let aikar = $state(initial.aikar)
	let jvmArgs = $state(initial.jvmArgs)
	let saving = $state(false)
	let message = $state("")
	let error = $state("")
	let cleaning = $state(false)
	let cleanReport = $state<CleanupReport | null>(null)
	/** Disk usage, computed on demand: walking every instance is not free. */
	let usage = $state<StorageUsage | null>(null)
	let usageLoading = $state(false)

	let javaPath = $state(initial.javaPath)
	let gameWidth = $state(initial.width)
	let gameHeight = $state(initial.height)
	let fullscreen = $state(initial.fullscreen)
	let discord = $state(initial.discord)

	/** What the launcher would actually run right now. */
	let javaInfo = $state<JavaInfo | null>(null)
	let javaError = $state("")

	// Modern versions need 21; that is the probe that matters in practice.
	$effect(() => {
		void ipc
			.resolveJava(21)
			.then((info) => {
				javaInfo = info
				javaError = ""
			})
			.catch((err) => {
				javaInfo = null
				javaError = (err as NimbusError).message ?? String(err)
			})
	})

	async function browseJava() {
		sound.play("click")
		try {
			const picked = await openDialog({
				multiple: false,
				filters: [{ name: "Java", extensions: ["exe"] }],
			})
			if (typeof picked === "string") javaPath = picked
		} catch (err) {
			error = (err as NimbusError).message ?? String(err)
		}
	}

	function fmtMemory(val: number): string {
		if (val >= 1024) return tf("{0} ГБ", (val / 1024).toFixed(val % 1024 === 0 ? 0 : 1))
		return tf("{0} МБ", val)
	}

	function fmtSize(bytes: number): string {
		if (bytes >= 1024 * 1024 * 1024) return tf("{0} ГБ", (bytes / (1024 * 1024 * 1024)).toFixed(2))
		if (bytes >= 1024 * 1024) return tf("{0} МБ", (bytes / (1024 * 1024)).toFixed(1))
		if (bytes >= 1024) return tf("{0} КБ", (bytes / 1024).toFixed(0))
		return tf("{0} Б", bytes)
	}

	async function save() {
		saving = true
		message = ""
		error = ""
		sound.play("click")
		try {
			const jvmLines = jvmArgs
				.split("\n")
				.map((l) => l.trim())
				.filter((l) => l.length > 0 && !l.startsWith("#"))
			const update: ConfigUpdate = {
				defaultMemoryMib: memory,
				defaultAikarFlags: aikar,
				defaultJvmArgs: jvmLines,
				// Empty string / zero explicitly clear the override server-side.
				javaPath,
				gameWidth: Number(gameWidth) || 0,
				gameHeight: Number(gameHeight) || 0,
				gameFullscreen: fullscreen,
				discordRpc: discord,
			}
			const next = await ipc.updateConfig(update)
			onconfig(next)
			message = t("Сохранено")
			sound.play("success")
			setTimeout(() => {
				message = ""
			}, 2000)
		} catch (err) {
			error = (err as NimbusError).message ?? String(err)
			sound.play("error")
		} finally {
			saving = false
		}
	}

	/** Opens the launcher's own log folder, for attaching to a bug report. */
	async function openLauncherLogs() {
		sound.play("click")
		try {
			await ipc.openLauncherLogsDir()
		} catch (err) {
			error = (err as NimbusError).message ?? String(err)
		}
	}

	/** Counts what every instance and the shared cache take up on disk. */
	async function loadUsage() {
		usageLoading = true
		sound.play("click")
		try {
			usage = await ipc.storageUsage()
		} catch (err) {
			error = (err as NimbusError).message ?? String(err)
		} finally {
			usageLoading = false
		}
	}

	/** Removes leftover installer marker files and broken library folders. */
	async function cleanup() {
		cleaning = true
		error = ""
		sound.play("click")
		try {
			const report = await ipc.cleanupShared()
			cleanReport = report
			if (report.removedFiles === 0) {
				toasts.info(t("Мусорных файлов не найдено"))
			} else {
				toasts.success(
					tf("Удалено файлов: {0} · освобождено {1}", report.removedFiles, fmtSize(report.freedBytes)),
				)
			}
		} catch (err) {
			error = (err as NimbusError).message ?? String(err)
		} finally {
			cleaning = false
		}
	}
</script>

<div class="pane">
	{#if message}
		<div class="flash anim-fade-up" role="status">
			<Icon name="check" size={14} strokeWidth={2} />
			{message}
		</div>
	{/if}
	{#if error}
		<div class="flash flash--error anim-fade-up" role="alert">
			<Icon name="alert" size={14} />
			{error}
		</div>
	{/if}

	<section class="card">
		<div class="card__head">
			<span class="card__title">{t("Внешний вид")}</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Язык интерфейса")}</span>
					<span class="row-hint">{t("Переключается сразу, без перезапуска")}</span>
				</div>
				<div class="chip-group" role="group" aria-label={t("Язык интерфейса")}>
					{#each LANGS as option (option.id)}
						<button
							class="chip"
							class:chip--active={i18n.current === option.id}
							type="button"
							aria-pressed={i18n.current === option.id}
							onclick={() => {
								sound.play("click")
								i18n.set(option.id)
							}}
						>
							{option.label}
						</button>
					{/each}
				</div>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Звуковые отклики")}</span>
					<span class="row-hint">{t("Тихие щелчки при наведении и нажатии")}</span>
				</div>
				<label class="toggle">
					<input
						type="checkbox"
						class="toggle__input"
						checked={sound.enabled}
						onchange={(e) => {
							const checked = (e.currentTarget as HTMLInputElement).checked
							sound.setEnabled(checked)
							if (checked) sound.play("toggle")
						}}
					/>
					<span class="toggle__track"></span>
					<span class="toggle-text">{sound.enabled ? t("Включён") : t("Выключен")}</span>
				</label>
			</div>

			{#if sound.enabled}
				<div class="row anim-fade-up">
					<div class="row-text">
						<label class="row-title" for="sound-volume">{t("Громкость")}</label>
					</div>
					<div class="slider-wrap">
						<input
							id="sound-volume"
							type="range"
							min="0"
							max="1"
							step="0.05"
							value={sound.volume}
							class="slider"
							oninput={(e) => sound.setVolume(Number((e.currentTarget as HTMLInputElement).value))}
							onchange={() => sound.play("click")}
						/>
						<span class="slider-val tnum">{Math.round(sound.volume * 100)}%</span>
					</div>
				</div>
			{/if}
		</div>
	</section>


	<section class="card">
		<div class="card__head">
			<span class="card__title">{t("Интеграции")}</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<span class="row-title">Discord Rich Presence</span>
					<span class="row-hint">
						{t("Показывать в Discord, в какую сборку вы играете, и время сессии")}
					</span>
				</div>
				<label class="toggle">
					<input
						type="checkbox"
						class="toggle__input"
						bind:checked={discord}
						onchange={() => sound.play("toggle")}
					/>
					<span class="toggle__track"></span>
					<span class="toggle-text">{discord ? t("Включён") : t("Выключен")}</span>
				</label>
			</div>
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">{t("Java и производительность")}</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<label class="row-title" for="memory">{t("Выделенная память")}</label>
					<span class="row-hint">{t("Общее значение для всех сборок")}</span>
				</div>
				<div class="slider-wrap">
					<input id="memory" type="range" min="512" max="32768" step="256" bind:value={memory} class="slider" />
					<span class="slider-val tnum">{fmtMemory(memory)}</span>
				</div>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Флаги Aikar (GC)")}</span>
					<span class="row-hint">{t("Оптимизированные параметры сборщика мусора")}</span>
				</div>
				<label class="toggle">
					<input type="checkbox" class="toggle__input" bind:checked={aikar} onchange={() => sound.play("toggle")} />
					<span class="toggle__track"></span>
					<span class="toggle-text">{aikar ? t("Включены") : t("Отключены")}</span>
				</label>
			</div>

			<div class="row row--stacked">
				<div class="row-text">
					<label class="row-title" for="jvm">{t("JVM аргументы")}</label>
					<span class="row-hint">{t("По одному аргументу на строку")}</span>
				</div>
				<textarea
					id="jvm"
					class="textarea"
					rows="5"
					placeholder="-XX:+UseG1GC&#10;-XX:MaxGCPauseMillis=200"
					bind:value={jvmArgs}
				></textarea>
			</div>

			<div class="row row--stacked">
				<div class="row-text">
					<label class="row-title" for="java-path">Java</label>
					<span class="row-hint">
						{#if javaError}
							Не удалось определить Java: {javaError}
						{:else if javaInfo}
							Сейчас используется:
							{javaInfo.isOverride
								? t("путь из настроек")
								: javaInfo.isManaged
									? t("runtime, скачанный лаунчером")
									: t("Java, найденная в системе")}
							<span class="path">{javaInfo.path}</span>
						{:else}
							Определение…
						{/if}
					</span>
				</div>
				<div class="control control--fill">
					<input
						id="java-path"
						class="input"
						type="text"
						spellcheck="false"
						placeholder={t("Автоматически (оставьте пустым)")}
						bind:value={javaPath}
					/>
					<button class="btn--sm" type="button" onclick={() => void browseJava()}>
						<Icon name="folder" size={14} />
						{t("Выбрать")}
					</button>
					{#if javaPath}
						<button class="btn--sm" type="button" onclick={() => (javaPath = "")}>
							{t("Сбросить")}
						</button>
					{/if}
				</div>
			</div>
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">{t("Окно игры")}</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Полноэкранный режим")}</span>
					<span class="row-hint">{t("Запускать игру во весь экран")}</span>
				</div>
				<label class="toggle">
					<input
						type="checkbox"
						class="toggle__input"
						bind:checked={fullscreen}
						onchange={() => sound.play("toggle")}
					/>
					<span class="toggle__track"></span>
					<span class="toggle-text">{fullscreen ? t("Включён") : t("Выключен")}</span>
				</label>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Размер окна")}</span>
					<span class="row-hint">
						{fullscreen
							? t("Не используется в полноэкранном режиме")
							: t("0 — оставить решение за Minecraft")}
					</span>
				</div>
				<div class="control">
					<input
						class="input input--num tnum"
						type="number"
						min="0"
						max="15360"
						step="16"
						aria-label={t("Ширина окна")}
						disabled={fullscreen}
						bind:value={gameWidth}
					/>
					<span class="times" aria-hidden="true">×</span>
					<input
						class="input input--num tnum"
						type="number"
						min="0"
						max="8640"
						step="16"
						aria-label={t("Высота окна")}
						disabled={fullscreen}
						bind:value={gameHeight}
					/>
				</div>
			</div>
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">{t("Обслуживание")}</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Автоматическая проверка")}</span>
					<span class="row-hint">{t("Проверять наличие новых версий в фоне при запуске")}</span>
				</div>
				<label class="toggle">
					<input
						type="checkbox"
						class="toggle__input"
						checked={updater.autoCheck}
						onchange={(e) => {
							sound.play("toggle")
							updater.setAutoCheck(e.currentTarget.checked)
						}}
					/>
					<span class="toggle__track"></span>
					<span class="toggle-text">{updater.autoCheck ? t("Включена") : t("Выключена")}</span>
				</label>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Обновления лаунчера")}</span>
					<span class="row-hint">
						{#if updater.status === "checking"}
							{t("Проверка…")}
						{:else if updater.available && updater.version}
							{tf("Доступно обновление: {0}", updater.version)}
						{:else}
							{t("Поиск свежей версии в репозитории проекта")}
						{/if}
					</span>
				</div>
				<div class="control">
					<button
						class="btn--sm"
						type="button"
						disabled={updater.checking || updater.downloading}
						onclick={() => void updater.check({ manual: true })}
					>
						<Icon name="refresh" size={14} />
						{updater.checking ? t("Проверка…") : t("Проверить обновления")}
					</button>
				</div>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Режим разработчика")}</span>
					<span class="row-hint">{t("Показывает служебный лог внизу окна")}</span>
				</div>
				<label class="toggle">
					<input type="checkbox" class="toggle__input" bind:checked={devMode} onchange={() => sound.play("toggle")} />
					<span class="toggle__track"></span>
					<span class="toggle-text">{devMode ? t("Включён") : t("Выключен")}</span>
				</label>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">{t("Логи лаунчера")}</span>
					<span class="row-hint">
						{t("Файл launcher.log рядом с логами игры: тихие сбои вроде повреждённого конфига или ошибок Rich Presence. Пригодится, если нужно приложить лог к сообщению о проблеме.")}
					</span>
				</div>
				<div class="control">
					<button class="btn--sm" type="button" onclick={() => void openLauncherLogs()}>
						<Icon name="folder" size={14} />
						{t("Открыть папку логов")}
					</button>
				</div>
			</div>

			<div class="row row--stacked">
				<div class="row-text">
					<span class="row-title">{t("Занятое место")}</span>
					<span class="row-hint">
						{t("Сколько занимают все сборки и общий кэш версий, библиотек и ассетов.")}
					</span>
					{#if usage}
						<ul class="usage">
							{#each usage.instances as item (item.id)}
								<li>
									<span class="usage-name">{item.name}</span>
									<span class="tnum">{fmtSize(item.bytes)}</span>
								</li>
							{/each}
							<li>
								<span class="usage-name">{t("Общий кэш")}</span>
								<span class="tnum">{fmtSize(usage.sharedBytes)}</span>
							</li>
							<li class="usage-total">
								<span class="usage-name">{t("Всего")}</span>
								<span class="tnum">{fmtSize(usage.totalBytes)}</span>
							</li>
						</ul>
					{/if}
				</div>
				<div class="control">
					<button
						class="btn--sm"
						type="button"
						disabled={usageLoading}
						onclick={() => void loadUsage()}
					>
						<Icon name="gauge" size={14} />
						{usageLoading ? t("Подсчёт…") : t("Посчитать")}
					</button>
				</div>
			</div>

			<div class="row row--stacked">
				<div class="row-text">
					<span class="row-title">{t("Очистка кэша")}</span>
					<span class="row-hint">
						{t("Удаляет служебные метки установщиков и повреждённые папки библиотек в общем кэше. Скачанные версии, моды и миры не затрагиваются.")}
					</span>
				</div>
				<div class="control">
					<button class="btn--sm" type="button" disabled={cleaning} onclick={() => void cleanup()}>
						<Icon name="refresh" size={14} />
						{cleaning ? t("Очистка…") : t("Очистить кэш")}
					</button>
					{#if cleanReport}
						<span class="row-hint tnum">
							Удалено: {cleanReport.removedFiles} · {fmtSize(cleanReport.freedBytes)}
						</span>
					{/if}
				</div>
			</div>
		</div>
	</section>

	<div class="save-bar">
		<span class="save-hint">{t("Изменения темы и звука применяются сразу")}</span>
		<button class="btn btn--play" type="button" disabled={saving} onclick={() => void save()}>
			{saving ? t("Сохранение…") : t("Сохранить настройки")}
		</button>
	</div>
</div>

<style>
	/* ── Storage usage ────────────────────────────────────────── */
	.usage {
		list-style: none;
		margin: var(--sp-2) 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: var(--fs-small);
		color: var(--text-2);
		max-width: 420px;
	}
	.usage li {
		display: flex;
		justify-content: space-between;
		gap: var(--sp-3);
	}
	.usage-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.usage-total {
		margin-top: var(--sp-1, 4px);
		padding-top: var(--sp-1, 4px);
		border-top: 1px solid var(--border-1);
		color: var(--text-1);
	}

	.pane {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
		width: 100%;
		max-width: 820px;
		margin: 0 auto;
		padding: var(--sp-6);
	}

	.flash {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		padding: var(--sp-3) var(--sp-4);
		border-radius: var(--r-md);
		font-size: var(--fs-small);
		color: var(--accent);
		background: var(--accent-soft);
		box-shadow: inset 0 0 0 1px var(--accent-border);
	}
	.flash--error {
		color: var(--danger);
		background: var(--danger-soft);
		box-shadow: inset 0 0 0 1px rgba(242, 85, 90, 0.3);
	}

	.card {
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.card__head {
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border-subtle);
	}

	.card__title {
		font-size: var(--fs-body);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}

	.card__body {
		padding: var(--sp-2) var(--sp-5) var(--sp-4);
	}

	.rows {
		display: flex;
		flex-direction: column;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-5);
		padding: var(--sp-4) 0;
		border-bottom: 1px solid var(--border-subtle);
	}
	.row:last-child {
		border-bottom: 0;
		padding-bottom: var(--sp-2);
	}
	.row--stacked {
		flex-direction: column;
		align-items: stretch;
		gap: var(--sp-3);
	}

	.row-text {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.row-title {
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
	}

	.row-hint {
		font-size: var(--fs-small);
		line-height: 1.5;
		color: var(--text-tertiary);
	}

	.control {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
	}
	.control--fill {
		flex: 1;
		width: auto;
	}

	.input--num {
		width: 96px;
	}

	.times {
		color: var(--text-tertiary);
	}

	.path {
		display: block;
		margin-top: 2px;
		font-family: var(--font-mono);
		font-size: var(--fs-micro);
		color: var(--text-disabled);
		word-break: break-all;
		user-select: text;
		-webkit-user-select: text;
	}

	.chip-group {
		flex: none;
		display: flex;
		gap: var(--sp-2);
	}

	.slider-wrap {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		width: 260px;
	}

	.slider-val {
		flex: none;
		min-width: 62px;
		text-align: right;
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
	}

	.toggle-text {
		font-size: var(--fs-small);
		color: var(--text-secondary);
	}

	.save-bar {
		position: sticky;
		bottom: 0;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
		padding: var(--sp-3) var(--sp-4);
		border-radius: var(--r-lg);
		background: var(--bg-surface);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-pop);
	}

	.save-hint {
		font-size: var(--fs-small);
		color: var(--text-tertiary);
	}
</style>
