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

	type Provider = "microsoft" | "ely" | "offline"

	let tab = $state<Provider>("microsoft")

	const NICK_RE = /^[A-Za-z0-9_]{1,16}$/

	// Built without template literals on purpose: these strings are plain URL
	// concatenation, so no brace syntax can creep into the markup.
	function avatarUrl(uuid: string, size: number): string {
		return "https://crafatar.com/avatars/" + uuid + "?size=" + String(size) + "&overlay"
	}

	function bodyUrl(uuid: string): string {
		return "https://vzge.me/full/512/" + uuid
	}

	// ── Microsoft accounts ───────────────────────────────────────────────────
	let accounts = $state<AccountInfo[]>([])
	let device = $state<DeviceCode | null>(null)
	let signingIn = $state(false)
	let authError = $state("")
	let switchingUuid = $state<string | null>(null)
	let removingUuid = $state<string | null>(null)
	let brokenAvatars = $state<Set<string>>(new Set())
	/** Set by cancelSignIn so the rejected login promise is not reported as a failure. */
	let cancelledSignIn = false

	const activeMs = $derived(accounts[0] ?? null)

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

	function shortUuid(uuid: string): string {
		return uuid.length > 13 ? uuid.slice(0, 8) + "…" + uuid.slice(-4) : uuid
	}

	async function signIn() {
		authError = ""
		signingIn = true
		cancelledSignIn = false
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
			// A user-requested cancel rejects this promise too; reporting it as a
			// failure would show an error for an action the user just took.
			if (cancelledSignIn) return
			authError = (err as NimbusError).message ?? String(err)
			sound.play("error")
		} finally {
			device = null
			signingIn = false
		}
	}

	async function cancelSignIn() {
		cancelledSignIn = true
		try {
			await ipc.cancelMsLogin()
		} catch {
			// Nothing to cancel is not an error worth reporting.
		}
		device = null
		signingIn = false
	}

	async function switchAccount(uuid: string) {
		if (switchingUuid || removingUuid) return
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
	/** Monotonic token: sign-in, switch and remove all trigger a skin reload. */
	let msSkinSeq = 0

	async function loadMicrosoftSkin() {
		const seq = ++msSkinSeq
		if (accounts.length === 0) {
			msSkin = null
			return
		}
		msSkinLoading = true
		try {
			const loaded = await ipc.getActiveMicrosoftSkin()
			// A newer reload started while this request was in flight: dropping it
			// keeps the panel showing the account that is actually active.
			if (seq !== msSkinSeq) return
			msSkin = loaded
			if (loaded) msModel = loaded.model
		} catch {
			if (seq !== msSkinSeq) return
			msSkin = null
		} finally {
			if (seq === msSkinSeq) msSkinLoading = false
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
	const activeOffline = $derived(offlineProfiles[0] ?? null)

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
		if (switchingNick || removingNick) return
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
		if (elySwitchingUuid || elyRemovingUuid) return
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
	<!-- ── Identity hero: who you are playing as, right now ─────────────── -->
	<section class="hero hero--identity anim-fade-up">
		{#if tab === "microsoft"}
			{#if activeMs}
				<div class="hero__figure">
					<SkinPreview src={msSkin?.url ?? null} scale={6} />
				</div>
				<div class="hero__meta">
					<span class="hero__kicker">{t("Сейчас играете как")}</span>
					<h2 class="hero__name">{activeMs.name}</h2>
					<div class="chips">
						<span class="chip chip--ok">
							<Icon name="shieldCheck" size={12} />
							{t("Лицензия Microsoft")}
						</span>
						{#if msSkinLoading}
							<span class="chip">{t("Загрузка скина…")}</span>
						{:else if msSkin}
							<span class="chip">{msModel === "slim" ? t("Тонкие руки") : t("Классика")}</span>
						{:else}
							<span class="chip">{t("Стандартный скин")}</span>
						{/if}
					</div>
					<button
						class="uuid"
						type="button"
						title={activeMs.uuid}
						onclick={() => void copyText(activeMs.uuid)}
					>
						<Icon name="copy" size={11} />
						UUID {shortUuid(activeMs.uuid)}
					</button>
				</div>
			{:else}
				<div class="hero__empty">
					<Icon name="user" size={22} />
					<div class="hero__meta">
						<h2 class="hero__name hero__name--muted">{t("Вход не выполнен")}</h2>
						<span class="hero__sub">
							{t("Войдите через Microsoft, чтобы играть с лицензией и своим скином.")}
						</span>
					</div>
				</div>
			{/if}
		{:else if tab === "ely"}
			{#if activeEly}
				<div class="hero__figure hero__figure--img">
					<img class="hero__body" src={bodyUrl(activeEly.uuid)} alt="" width="110" height="220" />
				</div>
				<div class="hero__meta">
					<span class="hero__kicker">{t("Сейчас играете как")}</span>
					<h2 class="hero__name">{activeEly.name}</h2>
					<div class="chips">
						<span class="chip chip--ok">
							<Icon name="globe" size={12} />
							Ely.by
						</span>
						<span class="chip">{t("Скин виден игрокам на Ely.by")}</span>
					</div>
					<button
						class="uuid"
						type="button"
						title={activeEly.uuid}
						onclick={() => void copyText(activeEly.uuid)}
					>
						<Icon name="copy" size={11} />
						UUID {shortUuid(activeEly.uuid)}
					</button>
				</div>
			{:else}
				<div class="hero__empty">
					<Icon name="globe" size={22} />
					<div class="hero__meta">
						<h2 class="hero__name hero__name--muted">{t("Ely.by не подключён")}</h2>
						<span class="hero__sub">
							{t("Бесплатные скины, видимые в мультиплеере без лицензии.")}
						</span>
					</div>
				</div>
			{/if}
		{:else if activeOffline}
			<div class="hero__figure hero__figure--mono">
				<span class="hero__initials">{initials(activeOffline.name)}</span>
			</div>
			<div class="hero__meta">
				<span class="hero__kicker">{t("Сейчас играете как")}</span>
				<h2 class="hero__name">{activeOffline.name}</h2>
				<div class="chips">
					<span class="chip chip--warn">
						<Icon name="user" size={12} />
						{t("Офлайн-режим")}
					</span>
					<span class="chip">{t("Только одиночная игра и пиратские серверы")}</span>
				</div>
			</div>
		{:else}
			<div class="hero__empty">
				<Icon name="user" size={22} />
				<div class="hero__meta">
					<h2 class="hero__name hero__name--muted">{t("Ников пока нет")}</h2>
					<span class="hero__sub">{t("Добавьте ник, чтобы запускать игру офлайн.")}</span>
				</div>
			</div>
		{/if}
	</section>

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
			{#if accounts.length > 0}<span class="tab__count">{accounts.length}</span>{/if}
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
			{#if elyAccounts.length > 0}<span class="tab__count">{elyAccounts.length}</span>{/if}
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
			{t("Офлайн")}
			{#if offlineProfiles.length > 0}<span class="tab__count">{offlineProfiles.length}</span>{/if}
		</button>
	</div>

	{#if tab === "microsoft"}
		{#if device}
			{@const code = device}
			<section class="card anim-fade-up">
				<div class="card__head">
					<span class="card__title">{t("Подтвердите вход")}</span>
					<span class="card__hint">
						{t("Окно можно не закрывать — лаунчер сам продолжит, когда вы подтвердите вход.")}
					</span>
				</div>
				<div class="card__body">
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
					<div class="waiting">
						<span class="spinner" aria-hidden="true"></span>
						<span class="row-hint">{t("Ожидание подтверждения…")}</span>
						<span class="spacer"></span>
						<button class="btn--sm" type="button" onclick={() => void ipc.openLoginPage()}>
							{t("Открыть страницу")}
						</button>
						<button class="btn--sm" type="button" onclick={() => void cancelSignIn()}>
							{t("Отменить")}
						</button>
					</div>
				</div>
			</section>
		{:else}
			<section class="card anim-fade-up">
				<div class="card__head card__head--row">
					<div class="card__headtext">
						<span class="card__title">{t("Аккаунты Microsoft")}</span>
						<span class="card__hint">{t("Нажмите на аккаунт, чтобы сделать его активным.")}</span>
					</div>
					<button class="btn--sm btn--on" type="button" disabled={signingIn} onclick={() => void signIn()}>
						<Icon name="plus" size={13} />
						{signingIn ? t("Вход…") : t("Добавить")}
					</button>
				</div>
				<div class="card__body">
					{#if accounts.length === 0}
						<div class="empty">
							<span class="row-hint">
								{t("Лицензия Minecraft: Java Edition проверяется автоматически, ник и скин берутся из аккаунта.")}
							</span>
							<button class="btn btn--play" type="button" disabled={signingIn} onclick={() => void signIn()}>
								<Icon name="user" size={14} />
								{signingIn ? t("Вход…") : t("Войти через Microsoft")}
							</button>
						</div>
					{:else}
						<ul class="list">
							{#each accounts as acc, i (acc.uuid)}
								<li class="item" class:item--on={i === 0}>
									<button
										class="item__pick"
										type="button"
										disabled={i === 0 || switchingUuid !== null}
										onclick={() => void switchAccount(acc.uuid)}
									>
										{#if brokenAvatars.has(acc.uuid)}
											<span class="avatar-fallback" aria-hidden="true">{initials(acc.name)}</span>
										{:else}
											<img
												class="avatar"
												src={avatarUrl(acc.uuid, 36)}
												alt=""
												width="36"
												height="36"
												onerror={() => (brokenAvatars = new Set([...brokenAvatars, acc.uuid]))}
											/>
										{/if}
										<span class="item__text">
											<span class="item__name">{acc.name}</span>
											<span class="item__sub mono">{shortUuid(acc.uuid)}</span>
										</span>
										{#if i === 0}
											<span class="badge badge--on">{t("Активен")}</span>
										{:else if switchingUuid === acc.uuid}
											<span class="badge">{t("Переключение…")}</span>
										{:else}
											<span class="badge badge--hover">{t("Сделать активным")}</span>
										{/if}
									</button>
									<button
										class="item__kill"
										type="button"
										title={t("Удалить")}
										aria-label={t("Удалить")}
										disabled={removingUuid === acc.uuid}
										onclick={() => void removeAccount(acc.uuid)}
									>
										<Icon name="trash" size={13} />
									</button>
								</li>
							{/each}
						</ul>
					{/if}

					{#if authError}
						<span class="auth-error" role="alert">{authError}</span>
					{/if}
				</div>
			</section>

			{#if accounts.length > 0}
				<section class="card anim-fade-up">
					<div class="card__head">
						<span class="card__title">{t("Скин")}</span>
						<span class="card__hint">
							{t("Настоящий скин аккаунта Microsoft — виден всем игрокам на любом сервере.")}
						</span>
					</div>
					<div class="card__body skin">
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
				</section>
			{/if}
		{/if}
	{:else if tab === "ely"}
		<section class="card anim-fade-up">
			<div class="card__head">
				<span class="card__title">{t("Аккаунты Ely.by")}</span>
				<span class="card__hint">
					{t("Бесплатный сервис скинов — видны всем игрокам, чей клиент также настроен на Ely.by.")}
				</span>
			</div>
			<div class="card__body">
				{#if elyAccounts.length > 0}
					<ul class="list">
						{#each elyAccounts as acc, i (acc.uuid)}
							<li class="item" class:item--on={i === 0}>
								<button
									class="item__pick"
									type="button"
									disabled={i === 0 || elySwitchingUuid !== null}
									onclick={() => void switchElyAccount(acc.uuid)}
								>
									{#if elyBrokenAvatars.has(acc.uuid)}
										<span class="avatar-fallback" aria-hidden="true">{initials(acc.name)}</span>
									{:else}
										<img
											class="avatar"
											src={avatarUrl(acc.uuid, 36)}
											alt=""
											width="36"
											height="36"
											onerror={() => (elyBrokenAvatars = new Set([...elyBrokenAvatars, acc.uuid]))}
										/>
									{/if}
									<span class="item__text">
										<span class="item__name">{acc.name}</span>
										<span class="item__sub mono">{shortUuid(acc.uuid)}</span>
									</span>
									{#if i === 0}
										<span class="badge badge--on">{t("Активен")}</span>
									{:else if elySwitchingUuid === acc.uuid}
										<span class="badge">{t("Переключение…")}</span>
									{:else}
										<span class="badge badge--hover">{t("Сделать активным")}</span>
									{/if}
								</button>
								<button
									class="item__kill"
									type="button"
									title={t("Удалить")}
									aria-label={t("Удалить")}
									disabled={elyRemovingUuid === acc.uuid}
									onclick={() => void removeElyAccount(acc.uuid)}
								>
									<Icon name="trash" size={13} />
								</button>
							</li>
						{/each}
					</ul>
				{/if}

				<div class="form">
					<span class="form__title">
						{elyAccounts.length > 0 ? t("Добавить ещё аккаунт") : t("Вход через Ely.by")}
					</span>
					<div class="form__row">
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
								{elyShowPassword ? t("Скрыть") : t("Показать")}
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
					<div class="form__links">
						<button class="link" type="button" onclick={() => void ipc.openUrl("https://ely.by")}>
							{t("Регистрация на ely.by")}
						</button>
						{#if activeEly}
							<button
								class="link"
								type="button"
								onclick={() => void ipc.openUrl("https://ely.by/profile/skins")}
							>
								{t("Управление скином")}
							</button>
						{/if}
					</div>
				</div>

				{#if elyError}
					<span class="auth-error" role="alert">{elyError}</span>
				{/if}
			</div>
		</section>
	{:else}
		<section class="card anim-fade-up">
			<div class="card__head">
				<span class="card__title">{t("Офлайн-ники")}</span>
				<span class="card__hint">
					{t("Игра без лицензии — как офлайн-режим обычного лаунчера.")}
				</span>
			</div>
			<div class="card__body">
				{#if offlineProfiles.length > 0}
					<ul class="list">
						{#each offlineProfiles as profile, i (profile.uuid)}
							<li class="item" class:item--on={i === 0}>
								<button
									class="item__pick"
									type="button"
									disabled={i === 0 || switchingNick !== null}
									onclick={() => void switchNickname(profile.name)}
								>
									<span class="avatar-fallback" aria-hidden="true">{initials(profile.name)}</span>
									<span class="item__text">
										<span class="item__name">{profile.name}</span>
										<span class="item__sub">{t("Офлайн-профиль")}</span>
									</span>
									{#if i === 0}
										<span class="badge badge--on">{t("Активен")}</span>
									{:else if switchingNick === profile.name}
										<span class="badge">{t("Переключение…")}</span>
									{:else}
										<span class="badge badge--hover">{t("Сделать активным")}</span>
									{/if}
								</button>
								<button
									class="item__kill"
									type="button"
									title={t("Удалить")}
									aria-label={t("Удалить")}
									disabled={removingNick === profile.name}
									onclick={() => void removeNickname(profile.name)}
								>
									<Icon name="trash" size={13} />
								</button>
							</li>
						{/each}
					</ul>
				{/if}

				<div class="form">
					<span class="form__title">{t("Добавить ник")}</span>
					<div class="form__row form__row--inline">
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
					<span class="form__hint">
						{t("Латинские буквы, цифры и подчёркивание, до 16 символов.")}
					</span>
				</div>

				{#if offlineError}
					<span class="auth-error" role="alert">{offlineError}</span>
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

	/* ── Identity hero ───────────────────────────────────────── */

	.hero {
		display: flex;
		align-items: center;
		gap: var(--sp-5);
		min-height: 132px;
		padding: var(--sp-4) var(--sp-5);
		border-radius: var(--r-lg);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-card);
	}

	.hero__figure {
		flex: none;
		display: grid;
		place-items: center;
		width: 104px;
		height: 104px;
		border-radius: var(--r-md);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		overflow: hidden;
	}

	.hero__figure--img {
		align-items: flex-start;
	}

	.hero__body {
		width: 100%;
		height: auto;
		image-rendering: pixelated;
	}

	.hero__figure--mono {
		background: var(--bg-inset);
	}

	.hero__initials {
		font-size: var(--fs-hero);
		font-weight: var(--fw-semibold);
		color: var(--text-secondary);
	}

	.hero__meta {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		min-width: 0;
	}

	.hero__kicker {
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	.hero__name {
		margin: 0;
		font-size: var(--fs-display);
		line-height: var(--lh-tight);
		font-weight: var(--fw-semibold);
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.hero__name--muted {
		color: var(--text-secondary);
		font-size: var(--fs-title);
	}

	.hero__sub {
		font-size: var(--fs-small);
		line-height: var(--lh-body);
		color: var(--text-tertiary);
		max-width: 52ch;
	}

	.hero__empty {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		color: var(--text-tertiary);
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--sp-2);
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		height: 22px;
		padding: 0 var(--sp-2);
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		font-size: var(--fs-micro);
		color: var(--text-secondary);
	}
	.chip--ok {
		color: var(--accent);
		background: var(--accent-soft);
	}
	.chip--warn {
		color: var(--warn);
		background: var(--warn-soft);
	}

	.uuid {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		width: fit-content;
		font-family: var(--font-mono);
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}
	.uuid:hover {
		color: var(--text-secondary);
	}

	/* ── Tabs ────────────────────────────────────────────────── */

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

	.tab__count {
		min-width: 17px;
		height: 17px;
		padding: 0 4px;
		border-radius: var(--r-full);
		background: var(--bg-inset);
		font-size: 10px;
		line-height: 17px;
		font-variant-numeric: tabular-nums;
		color: var(--text-tertiary);
	}

	/* ── Cards ───────────────────────────────────────────────── */

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
	.card__head--row {
		flex-direction: row;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
	}
	.card__headtext {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
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
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
		padding: var(--sp-4) var(--sp-5);
	}

	/* ── Account list ────────────────────────────────────────── */

	.list {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.item {
		position: relative;
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		border-radius: var(--r-md);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		transition: box-shadow var(--dur-fast) var(--ease-out);
	}
	.item--on {
		background: var(--accent-soft);
		box-shadow: inset 0 0 0 1px var(--accent-border, var(--accent));
	}

	.item__pick {
		flex: 1;
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		min-width: 0;
		padding: var(--sp-3);
		border-radius: var(--r-md);
		text-align: left;
	}
	.item__pick:not(:disabled) {
		cursor: pointer;
	}
	.item__pick:not(:disabled):hover {
		background: var(--bg-hover);
	}

	.item__text {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.item__name {
		font-size: var(--fs-body);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.item__sub {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}
	.mono {
		font-family: var(--font-mono);
	}

	.badge {
		flex: none;
		height: 22px;
		padding: 0 var(--sp-2);
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		font-size: var(--fs-micro);
		line-height: 22px;
		color: var(--text-tertiary);
	}
	.badge--on {
		color: var(--accent);
		background: transparent;
		font-weight: var(--fw-semibold);
	}
	.badge--hover {
		opacity: 0;
		transition: opacity var(--dur-fast) var(--ease-out);
	}
	.item__pick:hover .badge--hover,
	.item__pick:focus-visible .badge--hover {
		opacity: 1;
	}

	.item__kill {
		flex: none;
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		margin-right: var(--sp-2);
		border-radius: var(--r-sm);
		color: var(--text-tertiary);
		opacity: 0;
		transition:
			opacity var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out);
	}
	.item:hover .item__kill,
	.item__kill:focus-visible {
		opacity: 1;
	}
	.item__kill:hover {
		color: var(--danger);
		background: var(--danger-soft);
	}

	.avatar {
		flex: none;
		width: 36px;
		height: 36px;
		border-radius: var(--r-sm);
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
		border-radius: var(--r-sm);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		font-size: var(--fs-small);
		font-weight: var(--fw-semibold);
		color: var(--text-secondary);
	}

	/* ── Forms and states ────────────────────────────────────── */

	.empty {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--sp-3);
	}

	.form {
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
		padding-top: var(--sp-3);
		border-top: 1px solid var(--border-subtle);
	}
	.list + .form {
		margin-top: 0;
	}

	.form__title {
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-secondary);
	}

	.form__row {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}
	.form__row--inline {
		flex-direction: row;
		align-items: center;
	}
	.form__row .input {
		flex: 1;
		min-width: 0;
	}

	.form__hint {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.form__links {
		display: flex;
		gap: var(--sp-4);
	}

	.link {
		font-size: var(--fs-small);
		color: var(--accent);
	}
	.link:hover {
		text-decoration: underline;
	}

	.row-hint {
		font-size: var(--fs-small);
		line-height: var(--lh-body);
		color: var(--text-tertiary);
	}

	.auth-error {
		width: 100%;
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		color: var(--danger);
		background: var(--danger-soft);
	}

	/* ── Device code ─────────────────────────────────────────── */

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

	.waiting {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
	}
	.spacer {
		flex: 1;
	}

	.spinner {
		width: 13px;
		height: 13px;
		border-radius: var(--r-full);
		border: 2px solid var(--border-strong);
		border-top-color: var(--accent);
		animation: spin 700ms linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.spinner {
			animation: none;
		}
		.badge--hover,
		.item__kill {
			opacity: 1;
		}
	}

	/* ── Skin editor ─────────────────────────────────────────── */

	.skin {
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
