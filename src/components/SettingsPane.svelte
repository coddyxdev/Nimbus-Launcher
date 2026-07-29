<script lang="ts">
	import { untrack } from "svelte"
	import Icon from "./Icon.svelte"
	import {
		ipc,
		type AccountInfo,
		type CleanupReport,
		type Config,
		type ConfigUpdate,
		type DeviceCode,
		type JavaInfo,
		type NimbusError,
		type Theme,
	} from "$lib/ipc"
	import { open as openDialog } from "@tauri-apps/plugin-dialog"
	import { sound } from "$lib/sound.svelte"
	import {
		ACCENTS,
		applyAccent,
		applyTheme,
		readAccent,
		type AccentId,
	} from "$lib/theme"
	import { toasts } from "$lib/toast.svelte"

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
		theme: config.theme,
		nick: config.offlineUsername ?? "",
		memory: config.defaultMemoryMib,
		aikar: config.defaultAikarFlags,
		jvmArgs: config.defaultJvmArgs.join("\n"),
		javaPath: config.javaPath ?? "",
		width: config.gameWidth ?? 0,
		height: config.gameHeight ?? 0,
		fullscreen: config.gameFullscreen,
		discord: config.discordRpc,
		azureId: config.azureClientId ?? "",
	}))

	let theme = $state<Theme>(initial.theme)
	let nick = $state(initial.nick)
	let memory = $state(initial.memory)
	let aikar = $state(initial.aikar)
	let jvmArgs = $state(initial.jvmArgs)
	let saving = $state(false)
	let message = $state("")
	let error = $state("")
	let cleaning = $state(false)
	let cleanReport = $state<CleanupReport | null>(null)

	let javaPath = $state(initial.javaPath)
	let gameWidth = $state(initial.width)
	let gameHeight = $state(initial.height)
	let fullscreen = $state(initial.fullscreen)
	let accent = $state<AccentId>(readAccent())
	let discord = $state(initial.discord)

	// ── Microsoft account ────────────────────────────────────────────────────
	let azureId = $state(initial.azureId)
	let account = $state<AccountInfo | null>(null)
	/** Set while the user has a code to enter; null otherwise. */
	let device = $state<DeviceCode | null>(null)
	let signingIn = $state(false)
	let authError = $state("")

	$effect(() => {
		void ipc
			.getAccount()
			.then((found) => (account = found))
			.catch(() => {
				account = null
			})
	})

	/** Saves the client id, then runs the device-code flow to completion. */
	async function signIn() {
		authError = ""
		signingIn = true
		sound.play("click")
		try {
			onconfig(await ipc.setAzureClientId(azureId))
			device = await ipc.beginMsLogin()
			// Resolves only after the user finishes in the browser.
			account = await ipc.completeMsLogin()
			sound.play("success")
			toasts.success(`Вход выполнен: ${account.name}`)
		} catch (err) {
			authError = (err as NimbusError).message ?? String(err)
			sound.play("error")
		} finally {
			device = null
			signingIn = false
		}
	}

	async function cancelSignIn() {
		try {
			await ipc.cancelMsLogin()
		} catch {
			// Nothing to cancel is not an error worth reporting.
		}
		device = null
		signingIn = false
	}

	async function signOut() {
		sound.play("click")
		try {
			await ipc.signOut()
			account = null
			toasts.info("Аккаунт Microsoft отключён")
		} catch (err) {
			authError = (err as NimbusError).message ?? String(err)
		}
	}

	async function copyText(text: string) {
		try {
			await navigator.clipboard.writeText(text)
			toasts.success("Скопировано")
		} catch {
			toasts.error("Не удалось скопировать")
		}
	}

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

	/** Accent is a purely local preference, so it applies immediately. */
	function pickAccent(next: AccentId) {
		sound.play("toggle")
		accent = next
		applyAccent(next)
	}

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
		if (val >= 1024) return `${(val / 1024).toFixed(val % 1024 === 0 ? 0 : 1)} ГБ`
		return `${val} МБ`
	}

	function fmtSize(bytes: number): string {
		if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} ГБ`
		if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`
		if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} КБ`
		return `${bytes} Б`
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
				theme,
				defaultMemoryMib: memory,
				defaultAikarFlags: aikar,
				defaultJvmArgs: jvmLines,
				offlineUsername: nick || undefined,
				// Empty string / zero explicitly clear the override server-side.
				javaPath,
				gameWidth: Number(gameWidth) || 0,
				gameHeight: Number(gameHeight) || 0,
				gameFullscreen: fullscreen,
				discordRpc: discord,
			}
			const next = await ipc.updateConfig(update)
			applyTheme(next.theme)
			onconfig(next)
			message = "Сохранено"
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

	/** Theme applies instantly; no save button round trip. */
	async function setThemeInstant(next: Theme) {
		sound.play("toggle")
		theme = next
		applyTheme(next)
		try {
			onconfig(await ipc.setTheme(next))
		} catch {
			// Non-critical: the theme is already applied client-side.
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
				toasts.info("Мусорных файлов не найдено")
			} else {
				toasts.success(
					`Удалено файлов: ${report.removedFiles} · освобождено ${fmtSize(report.freedBytes)}`,
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
			<span class="card__title">Внешний вид</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<span class="row-title">Тема</span>
					<span class="row-hint">Применяется мгновенно</span>
				</div>
				<div class="chip-group">
					<button class="chip" class:chip--active={theme === "dark"} type="button" onclick={() => void setThemeInstant("dark")}>
						<Icon name="moon" size={13} />
						Тёмная
					</button>
					<button class="chip" class:chip--active={theme === "light"} type="button" onclick={() => void setThemeInstant("light")}>
						<Icon name="sun" size={13} />
						Светлая
					</button>
					<button class="chip" class:chip--active={theme === "system"} type="button" onclick={() => void setThemeInstant("system")}>
						<Icon name="monitor" size={13} />
						Системная
					</button>
				</div>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">Акцентный цвет</span>
					<span class="row-hint">Применяется сразу, хранится локально</span>
				</div>
				<div class="swatches" role="group" aria-label="Акцентный цвет">
					{#each ACCENTS as option (option.id)}
						<button
							class="swatch"
							class:swatch--on={accent === option.id}
							type="button"
							data-accent={option.id}
							title={option.label}
							aria-label={option.label}
							aria-pressed={accent === option.id}
							onclick={() => pickAccent(option.id)}
						></button>
					{/each}
				</div>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">Звуковые отклики</span>
					<span class="row-hint">Тихие щелчки при наведении и нажатии</span>
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
					<span class="toggle-text">{sound.enabled ? "Включён" : "Выключен"}</span>
				</label>
			</div>

			{#if sound.enabled}
				<div class="row anim-fade-up">
					<div class="row-text">
						<label class="row-title" for="sound-volume">Громкость</label>
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
			<span class="card__title">Аккаунт Microsoft</span>
		</div>
		<div class="card__body rows">
			{#if account}
				<div class="row">
					<div class="row-text">
						<span class="row-title">
							<span class="live-pip" aria-hidden="true"></span>
							{account.name}
						</span>
						<span class="row-hint">Лицензионный вход · UUID {account.uuid}</span>
					</div>
					<div class="control">
						<button class="btn--sm btn--danger" type="button" onclick={() => void signOut()}>
							Выйти
						</button>
					</div>
				</div>
			{:else if device}
				<!-- Narrowed once: the closures below would otherwise see it as nullable. -->
				{@const code = device}
				<div class="row row--stacked">
					<div class="row-text">
						<span class="row-title">Введите код на странице Microsoft</span>
						<span class="row-hint">
							Окно можно не закрывать — лаунчер сам продолжит, когда вы подтвердите вход.
						</span>
					</div>
					<div class="code-block">
						<div class="code-line">
							<span class="code-label">Код</span>
							<span class="code-value tnum">{code.userCode}</span>
							<button class="btn--sm" type="button" onclick={() => void copyText(code.userCode)}>
								<Icon name="copy" size={13} />
							</button>
						</div>
						<div class="code-line">
							<span class="code-label">Адрес</span>
							<span class="code-value code-value--url">{code.verificationUri}</span>
							<button
								class="btn--sm"
								type="button"
								onclick={() => void copyText(code.verificationUri)}
							>
								<Icon name="copy" size={13} />
							</button>
						</div>
					</div>
					<div class="control">
						<span class="row-hint">Ожидание подтверждения…</span>
						<button class="btn--sm" type="button" onclick={() => void cancelSignIn()}>
							Отменить
						</button>
					</div>
				</div>
			{:else}
				<div class="row row--stacked">
					<div class="row-text">
						<label class="row-title" for="azure-id">Azure Client ID</label>
						<span class="row-hint">
							Nimbus не содержит встроенного идентификатора: зарегистрируйте своё
							приложение по инструкции в <code>docs/AZURE_SETUP.md</code> и вставьте
							Application (client) ID сюда. Без него доступен только офлайн-режим.
						</span>
					</div>
					<div class="control control--fill">
						<input
							id="azure-id"
							class="input"
							type="text"
							spellcheck="false"
							placeholder="00000000-0000-0000-0000-000000000000"
							bind:value={azureId}
						/>
						<button
							class="btn btn--play"
							type="button"
							disabled={signingIn || !azureId.trim()}
							onclick={() => void signIn()}
						>
							<Icon name="user" size={14} />
							{signingIn ? "Вход…" : "Войти"}
						</button>
					</div>
				</div>
			{/if}

			{#if authError}
				<div class="row">
					<span class="auth-error" role="alert">{authError}</span>
				</div>
			{/if}
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">Интеграции</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<span class="row-title">Discord Rich Presence</span>
					<span class="row-hint">
						Показывать в Discord, в какую сборку вы играете, и время сессии
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
					<span class="toggle-text">{discord ? "Включён" : "Выключен"}</span>
				</label>
			</div>
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">Профиль</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<label class="row-title" for="nick">Офлайн-ник</label>
					<span class="row-hint">Используется для локального входа</span>
				</div>
				<div class="control control--input">
					<input id="nick" class="input" type="text" maxlength="16" placeholder="Steve" bind:value={nick} />
				</div>
			</div>
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">Java и производительность</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<label class="row-title" for="memory">Выделенная память</label>
					<span class="row-hint">Общее значение для всех сборок</span>
				</div>
				<div class="slider-wrap">
					<input id="memory" type="range" min="512" max="32768" step="256" bind:value={memory} class="slider" />
					<span class="slider-val tnum">{fmtMemory(memory)}</span>
				</div>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">Флаги Aikar (GC)</span>
					<span class="row-hint">Оптимизированные параметры сборщика мусора</span>
				</div>
				<label class="toggle">
					<input type="checkbox" class="toggle__input" bind:checked={aikar} onchange={() => sound.play("toggle")} />
					<span class="toggle__track"></span>
					<span class="toggle-text">{aikar ? "Включены" : "Отключены"}</span>
				</label>
			</div>

			<div class="row row--stacked">
				<div class="row-text">
					<label class="row-title" for="jvm">JVM аргументы</label>
					<span class="row-hint">По одному аргументу на строку</span>
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
								? "путь из настроек"
								: javaInfo.isManaged
									? "runtime, скачанный лаунчером"
									: "Java, найденная в системе"}
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
						placeholder="Автоматически (оставьте пустым)"
						bind:value={javaPath}
					/>
					<button class="btn--sm" type="button" onclick={() => void browseJava()}>
						<Icon name="folder" size={14} />
						Выбрать
					</button>
					{#if javaPath}
						<button class="btn--sm" type="button" onclick={() => (javaPath = "")}>
							Сбросить
						</button>
					{/if}
				</div>
			</div>
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">Окно игры</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<span class="row-title">Полноэкранный режим</span>
					<span class="row-hint">Запускать игру во весь экран</span>
				</div>
				<label class="toggle">
					<input
						type="checkbox"
						class="toggle__input"
						bind:checked={fullscreen}
						onchange={() => sound.play("toggle")}
					/>
					<span class="toggle__track"></span>
					<span class="toggle-text">{fullscreen ? "Включён" : "Выключен"}</span>
				</label>
			</div>

			<div class="row">
				<div class="row-text">
					<span class="row-title">Размер окна</span>
					<span class="row-hint">
						{fullscreen
							? "Не используется в полноэкранном режиме"
							: "0 — оставить решение за Minecraft"}
					</span>
				</div>
				<div class="control">
					<input
						class="input input--num tnum"
						type="number"
						min="0"
						max="15360"
						step="16"
						aria-label="Ширина окна"
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
						aria-label="Высота окна"
						disabled={fullscreen}
						bind:value={gameHeight}
					/>
				</div>
			</div>
		</div>
	</section>

	<section class="card">
		<div class="card__head">
			<span class="card__title">Обслуживание</span>
		</div>
		<div class="card__body rows">
			<div class="row">
				<div class="row-text">
					<span class="row-title">Режим разработчика</span>
					<span class="row-hint">Показывает служебный лог внизу окна</span>
				</div>
				<label class="toggle">
					<input type="checkbox" class="toggle__input" bind:checked={devMode} onchange={() => sound.play("toggle")} />
					<span class="toggle__track"></span>
					<span class="toggle-text">{devMode ? "Включён" : "Выключен"}</span>
				</label>
			</div>

			<div class="row row--stacked">
				<div class="row-text">
					<span class="row-title">Очистка кэша</span>
					<span class="row-hint">
						Удаляет служебные метки установщиков и повреждённые папки библиотек в общем
						кэше. Скачанные версии, моды и миры не затрагиваются.
					</span>
				</div>
				<div class="control">
					<button class="btn--sm" type="button" disabled={cleaning} onclick={() => void cleanup()}>
						<Icon name="refresh" size={14} />
						{cleaning ? "Очистка…" : "Очистить кэш"}
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
		<span class="save-hint">Изменения темы и звука применяются сразу</span>
		<button class="btn btn--play" type="button" disabled={saving} onclick={() => void save()}>
			{saving ? "Сохранение…" : "Сохранить настройки"}
		</button>
	</div>
</div>

<style>
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
	.control--input {
		width: 220px;
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

	.live-pip {
		display: inline-block;
		width: 7px;
		height: 7px;
		margin-right: 6px;
		border-radius: var(--r-full);
		background: var(--accent);
		vertical-align: middle;
	}

	.auth-error {
		width: 100%;
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		color: var(--danger);
		background: var(--danger-soft);
	}

	/* Device code: the two values the user has to carry to the browser. */
	.code-block {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		padding: var(--sp-3);
		border-radius: var(--r-md);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
	}

	.code-line {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
	}

	.code-label {
		flex: none;
		width: 48px;
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	.code-value {
		flex: 1;
		min-width: 0;
		font-family: var(--font-mono);
		font-size: var(--fs-title);
		font-weight: var(--fw-semibold);
		letter-spacing: 0.08em;
		color: var(--text-primary);
		user-select: text;
		-webkit-user-select: text;
	}

	.code-value--url {
		font-size: var(--fs-small);
		font-weight: var(--fw-regular);
		letter-spacing: 0;
		word-break: break-all;
	}

	/* Accent swatches read their own colour from the preset they represent. */	.swatches {
		flex: none;
		display: flex;
		gap: var(--sp-2);
	}

	.swatch {
		width: 26px;
		height: 26px;
		border-radius: var(--r-full);
		background: var(--accent);
		box-shadow:
			inset 0 1px 0 rgba(255, 255, 255, 0.25), 0 0 0 1px var(--border);
		transition:
			transform var(--dur-fast) var(--ease-spring),
			box-shadow var(--dur-fast) var(--ease-out);
	}
	.swatch:hover {
		transform: translateY(-1px) scale(1.06);
	}
	.swatch--on {
		box-shadow:
			inset 0 1px 0 rgba(255, 255, 255, 0.25), 0 0 0 2px var(--bg-raised),
			0 0 0 4px var(--accent);
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
