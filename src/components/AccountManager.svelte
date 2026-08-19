<script lang="ts">

	import { open as openDialog } from "@tauri-apps/plugin-dialog"
	import Icon from "./Icon.svelte"
	import SkinPreview from "./SkinPreview.svelte"
	import {
		ipc,
		type AccountInfo,
		type DeviceCode,
		type ElyAccountInfo,
		type NimbusError,
		type OfflineProfileInfo,
		type PublicSkin,
		type SkinModel,
	} from "$lib/ipc"
	import { sound } from "$lib/sound.svelte"
	import { toasts } from "$lib/toast.svelte"
	import { t, tf } from "$lib/i18n.svelte"

	let {
		/** Fired whenever the active Microsoft account changes (sign-in, switch, removal). */
		onchange,
	}: {
		onchange?: (account: AccountInfo | null) => void
	} = $props()

	let tab = $state<"microsoft" | "ely" | "offline">("microsoft")

	const NICK_RE = /^[A-Za-z0-9_]{1,16}$/

	// ── Microsoft accounts ───────────────────────────────────────────────────
	let accounts = $state<AccountInfo[]>([])
	let device = $state<DeviceCode | null>(null)
	let signingIn = $state(false)
	let authError = $state("")
	let switchingUuid = $state<string | null>(null)
	let removingUuid = $state<string | null>(null)
	let brokenAvatars = $state<Set<string>>(new Set())

	async function loadAccounts() {
		try {
			accounts = await ipc.listAccounts()
			onchange?.(accounts[0] ?? null)
		} catch {
			accounts = []
			onchange?.(null)
		}
	}

	function initials(name: string): string {
		return name.slice(0, 2).toUpperCase()
	}

	async function signIn() {
		authError = ""
		signingIn = true
		sound.play("click")
		try {
			device = await ipc.beginMsLogin()
			void ipc.openLoginPage().catch(() => {})
			const added = await ipc.completeMsLogin()
			await loadAccounts()
			void loadMicrosoftSkin()
			sound.play("success")
			toasts.success(tf("Вход выполнен: {0}", added.name))
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

	async function switchAccount(uuid: string) {
		switchingUuid = uuid
		sound.play("click")
		try {
			await ipc.switchAccount(uuid)
			await loadAccounts()
			void loadMicrosoftSkin()
		} catch (err) {
			authError = (err as NimbusError).message ?? String(err)
		} finally {
			switchingUuid = null
		}
	}

	async function removeAccount(uuid: string) {
		removingUuid = uuid
		sound.play("click")
		try {
			await ipc.removeAccount(uuid)
			await loadAccounts()
			void loadMicrosoftSkin()
			toasts.info(t("Аккаунт удалён"))
		} catch (err) {
			authError = (err as NimbusError).message ?? String(err)
		} finally {
			removingUuid = null
		}
	}

	async function copyText(text: string) {
		try {
			await navigator.clipboard.writeText(text)
			toasts.success(t("Скопировано"))
		} catch {
			toasts.error(t("Не удалось скопировать"))
		}
	}

	// ── Microsoft skin (real Mojang API — visible to everyone, everywhere) ──
	let msSkin = $state<PublicSkin | null>(null)
	let msSkinLoading = $state(false)
	let msModel = $state<SkinModel>("classic")
	let msSkinUrl = $state("")
	let msSkinBusy = $state(false)
	let msSkinError = $state("")

	async function loadMicrosoftSkin() {
		if (accounts.length === 0) {
			msSkin = null
			return
		}
		msSkinLoading = true
		try {
			msSkin = await ipc.getActiveMicrosoftSkin()
			if (msSkin) msModel = msSkin.model
		} catch {
			msSkin = null
		} finally {
			msSkinLoading = false
		}
	}

	async function applyMicrosoftSkinUrl() {
		const url = msSkinUrl.trim()
		if (!url || msSkinBusy) return
		msSkinBusy = true
		msSkinError = ""
		sound.play("click")
		try {
			await ipc.setMicrosoftSkinUrl(url, msModel)
			await loadMicrosoftSkin()
			msSkinUrl = ""
			sound.play("success")
			toasts.success(t("Скин обновлён — теперь виден всем и на любом сервере"))
		} catch (err) {
			msSkinError = (err as NimbusError).message ?? String(err)
			sound.play("error")
		} finally {
			msSkinBusy = false
		}
	}

	async function uploadMicrosoftSkinFile() {
		if (msSkinBusy) return
		const picked = await openDialog({
			multiple: false,
			filters: [{ name: "PNG", extensions: ["png"] }],
		})
		if (typeof picked !== "string") return
		msSkinBusy = true
		msSkinError = ""
		sound.play("click")
		try {
			await ipc.setMicrosoftSkinFile(picked, msModel)
			await loadMicrosoftSkin()
			sound.play("success")
			toasts.success(t("Скин обновлён — теперь виден всем и на любом сервере"))
		} catch (err) {
			msSkinError = (err as NimbusError).message ?? String(err)
			sound.play("error")
		} finally {
			msSkinBusy = false
		}
	}

	async function resetMicrosoftSkin() {
		if (msSkinBusy) return
		msSkinBusy = true
		msSkinError = ""
		sound.play("click")
		try {
			await ipc.resetMicrosoftSkin()
			await loadMicrosoftSkin()
			toasts.info(t("Скин сброшен на стандартный"))
		} catch (err) {
			msSkinError = (err as NimbusError).message ?? String(err)
		} finally {
			msSkinBusy = false
		}
	}

	// ── Ely.by accounts ─────────────────────────────────────────────────────
	let elyAccounts = $state<ElyAccountInfo[]>([])
	let elyUsername = $state("")
	let elyPassword = $state("")
	let elySigningIn = $state(false)
	let elyError = $state("")
	let elySwitchingUuid = $state<string | null>(null)
	let elyRemovingUuid = $state<string | null>(null)
	let elyShowPassword = $state(false)
	let elyBrokenAvatars = $state<Set<string>>(new Set())
	const activeEly = $derived(elyAccounts[0] ?? null)

	// ── Offline ("pirate") nicknames ─────────────────────────────────────────
	let offlineProfiles = $state<OfflineProfileInfo[]>([])
	let newNickname = $state("")
	let addingNickname = $state(false)
	let offlineError = $state("")
	let switchingNick = $state<string | null>(null)
	let removingNick = $state<string | null>(null)

	async function loadOfflineProfiles() {
		try {
			offlineProfiles = await ipc.listOfflineProfiles()
		} catch {
			offlineProfiles = []
		}
	}

	async function addNickname() {
		const name = newNickname.trim()
		if (!NICK_RE.test(name)) {
			offlineError = t("Ник может содержать только латинские буквы, цифры и подчёркивание (1–16).")
			sound.play("warn")
			return
		}
		addingNickname = true
		offlineError = ""
		sound.play("click")
		try {
			await ipc.addOfflineProfile(name)
			newNickname = ""
			await loadOfflineProfiles()
			sound.play("success")
			toasts.success(tf("Добавлен: {0}", name))
		} catch (err) {
			offlineError = (err as NimbusError).message ?? String(err)
			sound.play("error")
		} finally {
			addingNickname = false
		}
	}

	async function switchNickname(name: string) {
		switchingNick = name
		sound.play("click")
		try {
			await ipc.switchOfflineProfile(name)
			await loadOfflineProfiles()
		} catch (err) {
			offlineError = (err as NimbusError).message ?? String(err)
		} finally {
			switchingNick = null
		}
	}

	async function removeNickname(name: string) {
		removingNick = name
		sound.play("click")
		try {
			await ipc.removeOfflineProfile(name)
			await loadOfflineProfiles()
			toasts.info(t("Ник удалён"))
		} catch (err) {
			offlineError = (err as NimbusError).message ?? String(err)
		} finally {
			removingNick = null
		}
	}

	async function loadElyAccounts() {
		try {
			elyAccounts = await ipc.listElyAccounts()
		} catch {
			elyAccounts = []
		}
	}

	async function elySignIn() {
		const user = elyUsername.trim()
		const pass = elyPassword.trim()
		if (!user || !pass) {
			elyError = t("Введите логин и пароль Ely.by")
			sound.play("warn")
			return
		}
		elySigningIn = true
		elyError = ""
		sound.play("click")
		try {
			const info = await ipc.elySignIn(user, pass)
			elyUsername = ""
			elyPassword = ""
			await loadElyAccounts()
			sound.play("success")
			toasts.success(tf("Вход выполнен: {0}", info.name))
		} catch (err) {
			elyError = (err as NimbusError).message ?? String(err)
			sound.play("error")
		} finally {
			elySigningIn = false
		}
	}

	async function switchElyAccount(uuid: string) {
		elySwitchingUuid = uuid
		sound.play("click")
		try {
			await ipc.switchElyAccount(uuid)
			await loadElyAccounts()
		} catch (err) {
			elyError = (err as NimbusError).message ?? String(err)
		} finally {
			elySwitchingUuid = null
		}
	}

	async function removeElyAccount(uuid: string) {
		elyRemovingUuid = uuid
		sound.play("click")
		try {
			await ipc.removeElyAccount(uuid)
			await loadElyAccounts()
			toasts.info(t("Аккаунт удалён"))
		} catch (err) {
			elyError = (err as NimbusError).message ?? String(err)
		} finally {
			elyRemovingUuid = null
		}
	}



	$effect(() => {
		void loadAccounts().then(loadMicrosoftSkin)
		void loadOfflineProfiles()
		void loadElyAccounts()
	})
</script>

<div class="pane">
	<div class="tabs" role="tablist">
		<button
			class="tab"
			class:tab--on={tab === "microsoft"}
			type="button"
			role="tab"
			aria-selected={tab === "microsoft"}
			onclick={() => {
				sound.play("tab")
				tab = "microsoft"
			}}
		>
			<Icon name="shieldCheck" size={14} />
			{t("Microsoft")}
		</button>
		<button
			class="tab"
			class:tab--on={tab === "ely"}
			type="button"
			role="tab"
			aria-selected={tab === "ely"}
			onclick={() => {
				sound.play("tab")
				tab = "ely"
			}}
		>
			<Icon name="globe" size={14} />
			Ely.by
		</button>
		<button
			class="tab"
			class:tab--on={tab === "offline"}
			type="button"
			role="tab"
			aria-selected={tab === "offline"}
			onclick={() => {
				sound.play("tab")
				tab = "offline"
			}}
		>
			<Icon name="user" size={14} />
			{t("Пиратские ники")}
		</button>
	</div>

	{#if tab === "microsoft"}
		<section class="card anim-fade-up">
			<div class="card__head">
				<span class="card__title">{t("Аккаунты Microsoft")}</span>
			</div>
			<div class="card__body rows">
				{#if device}
					<!-- Narrowed once: the closures below would otherwise see it as nullable. -->
					{@const code = device}
					<div class="row row--stacked">
						<div class="row-text">
							<span class="row-title">{t("Введите код на странице Microsoft")}</span>
							<span class="row-hint">
								{t("Окно можно не закрывать — лаунчер сам продолжит, когда вы подтвердите вход.")}
							</span>
						</div>
						<div class="code-block">
							<div class="code-line">
								<span class="code-label">{t("Код")}</span>
								<span class="code-value tnum">{code.userCode}</span>
								<button class="btn--sm" type="button" onclick={() => void copyText(code.userCode)}>
									<Icon name="copy" size={13} />
								</button>
							</div>
							<div class="code-line">
								<span class="code-label">{t("Адрес")}</span>
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
							<span class="row-hint">{t("Ожидание подтверждения…")}</span>
							<button class="btn--sm" type="button" onclick={() => void ipc.openLoginPage()}>
								{t("Открыть страницу")}
							</button>
							<button class="btn--sm" type="button" onclick={() => void cancelSignIn()}>
								{t("Отменить")}
							</button>
						</div>
					</div>
				{:else}
					{#if accounts.length > 0}
						<div class="rows">
							{#each accounts as acc, i (acc.uuid)}
								<div class="row">
									<div class="row-identity">
										{#if brokenAvatars.has(acc.uuid)}
											<span class="avatar-fallback" aria-hidden="true">{initials(acc.name)}</span>
										{:else}
											<img
												class="avatar"
												src={`https://crafatar.com/avatars/${acc.uuid}?size=36&overlay`}
												alt=""
												width="36"
												height="36"
												onerror={() => (brokenAvatars = new Set([...brokenAvatars, acc.uuid]))}
											/>
										{/if}
										<div class="row-text">
											<span class="row-title">
												{#if i === 0}<span class="live-pip" aria-hidden="true"></span>{/if}
												{acc.name}
											</span>
											<span class="row-hint">
												{i === 0 ? t("Активен сейчас") : t("Не активен")} · UUID {acc.uuid}
											</span>
										</div>
									</div>
									<div class="control">
										{#if i !== 0}
											<button
												class="btn--sm"
												type="button"
												disabled={switchingUuid === acc.uuid}
												onclick={() => void switchAccount(acc.uuid)}
											>
												{switchingUuid === acc.uuid ? t("Переключение…") : t("Сделать активным")}
											</button>
										{/if}
										<button
											class="btn--sm btn--danger"
											type="button"
											disabled={removingUuid === acc.uuid}
											onclick={() => void removeAccount(acc.uuid)}
										>
											{removingUuid === acc.uuid ? t("Удаление…") : t("Удалить")}
										</button>
									</div>
								</div>
							{/each}
						</div>
					{/if}

					<div class="row">
						<div class="row-text">
							<span class="row-title">{t("Вход через Microsoft")}</span>
							<span class="row-hint">
								{accounts.length > 0
									? t("Можно войти ещё одним аккаунтом Microsoft.")
									: t("Лицензия Minecraft: Java Edition проверяется автоматически, ник и скин берутся из аккаунта.")}
							</span>
						</div>
						<div class="control">
							<button
								class="btn btn--play"
								type="button"
								disabled={signingIn}
								onclick={() => void signIn()}
							>
								<Icon name="user" size={14} />
								{signingIn
									? t("Вход…")
									: accounts.length > 0
										? t("Добавить аккаунт")
										: t("Войти через Microsoft")}
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

		{#if accounts.length > 0 && !device}
			<section class="card anim-fade-up">
				<div class="card__head">
					<span class="card__title">{t("Скин")}</span>
					<span class="card__hint">
						{t("Настоящий скин аккаунта Microsoft — виден всем игрокам на любом сервере.")}
					</span>
				</div>
				<div class="card__body">
					<div class="skin-editor">
						<SkinPreview src={msSkin?.url ?? null} scale={7} />
						<div class="skin-controls">
							<div class="model-picker" role="radiogroup" aria-label={t("Модель")}>
								<button
									class="model-btn"
									class:model-btn--on={msModel === "classic"}
									type="button"
									role="radio"
									aria-checked={msModel === "classic"}
									onclick={() => (msModel = "classic")}
								>
									{t("Классика")}
								</button>
								<button
									class="model-btn"
									class:model-btn--on={msModel === "slim"}
									type="button"
									role="radio"
									aria-checked={msModel === "slim"}
									onclick={() => (msModel = "slim")}
								>
									{t("Тонкие руки")}
								</button>
							</div>

							<div class="skin-url-row">
								<input
									class="input"
									type="text"
									placeholder={t("Ссылка на PNG-скин")}
									bind:value={msSkinUrl}
									disabled={msSkinBusy}
									onkeydown={(e) => {
										if (e.key === "Enter") void applyMicrosoftSkinUrl()
									}}
								/>
								<button
									class="btn--sm"
									type="button"
									disabled={msSkinBusy || !msSkinUrl.trim()}
									onclick={() => void applyMicrosoftSkinUrl()}
								>
									{t("Применить")}
								</button>
							</div>

							<div class="skin-actions">
								<button
									class="btn--sm btn--on"
									type="button"
									disabled={msSkinBusy}
									onclick={() => void uploadMicrosoftSkinFile()}
								>
									<Icon name="upload" size={13} />
									{t("Загрузить с ПК")}
								</button>
								<button
									class="btn--sm"
									type="button"
									disabled={msSkinBusy || (!msSkin && !msSkinLoading)}
									onclick={() => void resetMicrosoftSkin()}
								>
									{t("Сбросить")}
								</button>
							</div>

							{#if msSkinError}
								<span class="auth-error" role="alert">{msSkinError}</span>
							{/if}
						</div>
					</div>
				</div>
			</section>
		{/if}
	{:else if tab === "ely"}
		<section class="card anim-fade-up">
			<div class="card__head">
				<span class="card__title">Ely.by</span>
				<span class="card__hint">
					{t("Бесплатный сервис скинов — видны всем игрокам, чей клиент также настроен на Ely.by.")}
				</span>
			</div>
			<div class="card__body rows">
				{#if elyAccounts.length > 0}
					<div class="rows">
						{#each elyAccounts as acc, i (acc.uuid)}
							<div class="row">
								<div class="row-identity">
									{#if elyBrokenAvatars.has(acc.uuid)}
										<span class="avatar-fallback" aria-hidden="true">{initials(acc.name)}</span>
									{:else}
										<img
											class="avatar"
											src={`https://crafatar.com/avatars/${acc.uuid}?size=36&overlay`}
											alt=""
											width="36"
											height="36"
											onerror={() => (elyBrokenAvatars = new Set([...elyBrokenAvatars, acc.uuid]))}
										/>
									{/if}
									<div class="row-text">
										<span class="row-title">
											{#if i === 0}<span class="live-pip" aria-hidden="true"></span>{/if}
											{acc.name}
										</span>
										<span class="row-hint">
											{i === 0 ? t("Активен сейчас") : t("Не активен")} · UUID {acc.uuid}
										</span>
									</div>
								</div>
								<div class="control">
									{#if i !== 0}
										<button
											class="btn--sm"
											type="button"
											disabled={elySwitchingUuid === acc.uuid}
											onclick={() => void switchElyAccount(acc.uuid)}
										>
											{elySwitchingUuid === acc.uuid ? t("Переключение…") : t("Сделать активным")}
										</button>
									{/if}
									<button
										class="btn--sm btn--danger"
										type="button"
										disabled={elyRemovingUuid === acc.uuid}
										onclick={() => void removeElyAccount(acc.uuid)}
									>
										{elyRemovingUuid === acc.uuid ? t("Удаление…") : t("Удалить")}
									</button>
								</div>
							</div>
						{/each}					</div>
				{/if}

				{#if activeEly}
					<div class="ely-skin-preview">
						<div class="skin-editor">
							<div class="ely-skin-body">
								<img
									class="ely-skin-img"
									src={`https://vzge.me/full/512/${activeEly.uuid}`}
									alt=""
									width="128"
									height="256"
								/>
							</div>
							<div class="skin-controls">
								<span class="row-title">{tf("Скин: {0}", activeEly.name)}</span>
								<span class="row-hint">
									{t("Скин управляется на сайте ely.by. Нажмите кнопку ниже, чтобы загрузить или изменить скин.")}
								</span>
								<div class="skin-actions" style="margin-top: var(--sp-2);">
									<button
										class="btn btn--play"
										type="button"
										onclick={() => void ipc.openUrl("https://ely.by/profile/skins")}
									>
										<Icon name="upload" size={13} />
										{t("Управление скином")}
									</button>
								</div>
							</div>
						</div>
					</div>
				{/if}

				<div class="row row--stacked">
					<div class="row-text">
						<span class="row-title">{t("Вход через Ely.by")}</span>
						<span class="row-hint">
							{elyAccounts.length > 0
								? t("Можно войти ещё одним аккаунтом Ely.by.")
								: t("Зарегистрируйтесь на ely.by и войдите здесь, чтобы скины были видны в мультиплеере.")}
						</span>
					</div>
				</div>

				<div class="row row--stacked">
					<div class="control control--fill">
						<input
							class="input"
							type="text"
							placeholder={t("Логин или email")}
							bind:value={elyUsername}
							disabled={elySigningIn}
							onkeydown={(e) => {
								if (e.key === "Enter") void elySignIn()
							}}
						/>
						<div class="password-wrap">
							<input
								class="input"
								type={elyShowPassword ? "text" : "password"}
								placeholder={t("Пароль")}
								bind:value={elyPassword}
								disabled={elySigningIn}
								onkeydown={(e) => {
									if (e.key === "Enter") void elySignIn()
								}}
							/>
							<button
								class="btn--sm"
								type="button"
								onclick={() => (elyShowPassword = !elyShowPassword)}
								aria-label={elyShowPassword ? t("Скрыть пароль") : t("Показать пароль")}
							>
								<Icon name={elyShowPassword ? "trash" : "edit"} size={13} />
							</button>
						</div>
						<button
							class="btn btn--play"
							type="button"
							disabled={elySigningIn || !elyUsername.trim() || !elyPassword.trim()}
							onclick={() => void elySignIn()}
						>
							<Icon name="user" size={14} />
							{elySigningIn ? t("Вход…") : t("Войти")}
						</button>
					</div>
				</div>

				{#if elyError}
					<div class="row">
						<span class="auth-error" role="alert">{elyError}</span>
					</div>
				{/if}
			</div>
		</section>

		<section class="card anim-fade-up">
			<div class="card__head">
				<span class="card__title">{t("Как это работает")}</span>
				<span class="card__hint">
					{t("Ely.by использует authlib-injector — тот же открытый протокол, что и TLauncher, XMCL и другие лаунчеры. Скин виден только тем, кто тоже играет через Ely.by или совместимый сервис.")}
				</span>
			</div>
			<div class="card__body">
				<div class="skin-controls">
					<span class="row-hint">
						{t("Загрузите скин на ely.by → он автоматически станет виден в мультиплеере. В лаунчере ничего настраивать не нужно.")}
					</span>
					<div class="row" style="margin-top: var(--sp-3);">
						<button
							class="btn btn--play"
							type="button"
							onclick={() => void ipc.openUrl("https://ely.by")}
						>
							<Icon name="globe" size={14} />
							{t("Открыть ely.by")}
						</button>
					</div>
				</div>
			</div>
		</section>
	{:else}
		<section class="card anim-fade-up">
			<div class="card__head">
				<span class="card__title">{t("Пиратские ники")}</span>
				<span class="card__hint">
					{t("Игра без лицензии — как офлайн-режим обычного лаунчера.")}
				</span>
			</div>
			<div class="card__body rows">
				{#if offlineProfiles.length > 0}
					<div class="rows">
						{#each offlineProfiles as profile, i (profile.uuid)}
							<div class="row">
								<div class="row-identity">
									<span class="avatar-fallback" aria-hidden="true">{initials(profile.name)}</span>
									<div class="row-text">
										<span class="row-title">
											{#if i === 0}<span class="live-pip" aria-hidden="true"></span>{/if}
											{profile.name}
										</span>
										<span class="row-hint">
											{i === 0 ? t("Активен сейчас") : t("Не активен")}
										</span>
									</div>
								</div>
								<div class="control">
									{#if i !== 0}
										<button
											class="btn--sm"
											type="button"
											disabled={switchingNick === profile.name}
											onclick={() => void switchNickname(profile.name)}
										>
											{switchingNick === profile.name ? t("Переключение…") : t("Сделать активным")}
										</button>
									{/if}
									<button
										class="btn--sm btn--danger"
										type="button"
										disabled={removingNick === profile.name}
										onclick={() => void removeNickname(profile.name)}
									>
										{removingNick === profile.name ? t("Удаление…") : t("Удалить")}
									</button>
								</div>
							</div>
						{/each}
					</div>
				{/if}

				<div class="row row--stacked">
					<div class="row-text">
						<span class="row-title">{t("Добавить ник")}</span>
						<span class="row-hint">
							{t("Игра запустится офлайн под этим ником.")}
						</span>
					</div>
					<div class="control control--fill">
						<input
							class="input"
							type="text"
							maxlength="16"
							placeholder="Steve"
							bind:value={newNickname}
							disabled={addingNickname}
							onkeydown={(e) => {
								if (e.key === "Enter") void addNickname()
							}}
						/>
						<button
							class="btn--sm btn--on"
							type="button"
							disabled={addingNickname || !newNickname.trim()}
							onclick={() => void addNickname()}
						>
							<Icon name="plus" size={13} />
							{addingNickname ? t("Добавление…") : t("Добавить")}
						</button>
					</div>
				</div>

				{#if offlineError}
					<div class="row">
						<span class="auth-error" role="alert">{offlineError}</span>
					</div>
				{/if}
			</div>
		</section>
	{/if}
</div>

<style>
	.pane {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	.tabs {
		display: flex;
		gap: var(--sp-2);
		padding: 4px;
		border-radius: var(--r-md);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		width: fit-content;
	}

	.tab {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 30px;
		padding: 0 var(--sp-4);
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-secondary);
	}
	.tab--on {
		color: var(--text-primary);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--shadow-sm);
	}

	.card {
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.card__head {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border-subtle);
	}

	.card__title {
		font-size: var(--fs-body);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
	}

	.card__hint {
		font-size: var(--fs-small);
		color: var(--text-tertiary);
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

	.row-identity {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		min-width: 0;
	}

	.avatar {
		flex: none;
		width: 36px;
		height: 36px;
		border-radius: var(--r-md);
		box-shadow: var(--edge-ring);
		image-rendering: pixelated;
		object-fit: cover;
	}

	.avatar-fallback {
		display: grid;
		flex: none;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: var(--r-md);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		font-size: var(--fs-small);
		font-weight: var(--fw-semibold);
		color: var(--text-secondary);
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
	.control--fill .input {
		flex: 1;
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

	/* ── Skin editor ─────────────────────────────────────────── */

	.skin-editor {
		display: flex;
		align-items: flex-start;
		gap: var(--sp-5);
	}

	.skin-controls {
		display: flex;
		flex: 1;
		min-width: 0;
		flex-direction: column;
		gap: var(--sp-3);
	}

	.model-picker {
		display: flex;
		gap: 4px;
		padding: 4px;
		width: fit-content;
		border-radius: var(--r-md);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
	}

	.model-btn {
		height: 28px;
		padding: 0 var(--sp-3);
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		color: var(--text-secondary);
	}
	.model-btn--on {
		color: var(--text-primary);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring);
	}

	.skin-url-row {
		display: flex;
		gap: var(--sp-2);
	}
	.skin-url-row .input {
		flex: 1;
		min-width: 0;
	}

	.skin-actions {
		display: flex;
		gap: var(--sp-2);
	}

	.ely-skin-preview {
		padding: var(--sp-3) 0;
		border-bottom: 1px solid var(--border-subtle);
	}

	.ely-skin-body {
		flex: none;
		width: 128px;
		border-radius: var(--r-md);
		overflow: hidden;
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
	}

	.ely-skin-img {
		width: 100%;
		height: auto;
		image-rendering: pixelated;
	}

	.password-wrap {
		display: flex;
		gap: var(--sp-2);
		align-items: stretch;
	}
	.password-wrap .input {
		flex: 1;
		min-width: 0;
	}
</style>
