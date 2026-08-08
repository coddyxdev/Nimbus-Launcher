<script lang="ts">
	import { ipc, type Config } from "$lib/ipc"
	import { sound } from "$lib/sound.svelte"
	import { i18n, t, LANGS } from "$lib/i18n.svelte"
	import Icon from "./Icon.svelte"

	let {
		authUnavailable,
		ondone,
	}: {
		authUnavailable: boolean
		ondone: (config: Config) => void
	} = $props()

	// Two screens, no more.
	let step = $state<1 | 2>(1)
	let username = $state("")
	let error = $state("")
	let busy = $state(false)

	const USERNAME_RE = /^[A-Za-z0-9_]{1,16}$/

	async function finish() {
		if (busy) return
		if (!USERNAME_RE.test(username)) {
			error = t("Ник может содержать только латинские буквы, цифры и подчёркивание (1–16).")
			sound.play("warn")
			return
		}
		busy = true
		error = ""
		try {
			await ipc.setOfflineUsername(username)
			const config = await ipc.completeOnboarding()
			sound.play("success")
			ondone(config)
		} catch (err) {
			error = (err as { message?: string }).message ?? String(err)
			sound.play("error")
		} finally {
			busy = false
		}
	}
</script>

<div class="wrap">
	<div class="panel anim-scale-in">
		<!-- Language first: the rest of onboarding should already read in the
		     user's language. Defaults to English on a fresh install. -->
		<div class="lang" role="group" aria-label={t("Язык")}>
			{#each LANGS as option (option.id)}
				<button
					class="lang-btn"
					class:lang-btn--on={i18n.current === option.id}
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

		<div class="steps" aria-hidden="true">
			<span class="step-dot" class:step-dot--on={step >= 1}></span>
			<span class="step-line" class:step-line--on={step >= 2}></span>
			<span class="step-dot" class:step-dot--on={step >= 2}></span>
		</div>

		{#if step === 1}
			<span class="mark">
				<img src="/logo.png" alt="" aria-hidden="true" draggable="false" />
			</span>
			<p class="eyebrow">{t("Шаг 1 из 2")}</p>
			<h2 class="h">Nimbus Client</h2>
			<p class="lede">
				{t(
					"Лаунчер для Minecraft с изолированными сборками, управлением модами и живой консолью. Данные хранятся локально.",
				)}
			</p>
			<ul class="features">
				<li><Icon name="cube" size={14} />{t("Изолированные сборки и версии")}</li>
				<li><Icon name="package" size={14} />{t("Моды из каталога Modrinth")}</li>
				<li><Icon name="terminal" size={14} />{t("Живая консоль и краш-репорты")}</li>
			</ul>
			<button
				class="btn btn--play wide"
				type="button"
				onclick={() => {
					sound.play("click")
					step = 2
				}}
			>
				{t("Дальше")}
			</button>
		{:else}
			<p class="eyebrow">{t("Шаг 2 из 2")}</p>
			<h2 class="h">{t("Аккаунт")}</h2>

			<div class="field">
				<label class="field-label" for="nick">{t("Ник для офлайн-режима")}</label>
				<input
					id="nick"
					class="input"
					type="text"
					maxlength="16"
					pattern={"[A-Za-z0-9_]{1,16}"}
					spellcheck="false"
					autocomplete="off"
					placeholder="Steve"
					bind:value={username}
					onkeydown={(e) => {
						if (e.key === "Enter") void finish()
					}}
				/>
				<p class="hint">{t("Латинские буквы, цифры и подчёркивание, до 16 символов.")}</p>
			</div>

			<div class="ms">
				<div class="ms-text">
					<span class="ms-title">{t("Вход через Microsoft")}</span>
					<span class="ms-sub">
						{authUnavailable
							? t("Нужен свой Azure Client ID — см. docs/AZURE_SETUP.md")
							: t("Client ID задан: войти можно в настройках")}
					</span>
				</div>
				<!-- Sign-in lives in Settings: it needs a client id first, and the
				     device-code step should not block onboarding. -->
				<span class="ms-note">{t("Настройки → Аккаунт")}</span>
			</div>

			{#if error}
				<p class="err anim-fade-up">{error}</p>
			{/if}

			<div class="row">
				<button
					class="btn btn--ghost"
					type="button"
					onclick={() => {
						sound.play("click")
						step = 1
					}}
				>
					{t("Назад")}
				</button>
				<button
					class="btn btn--play"
					type="button"
					disabled={busy || !USERNAME_RE.test(username)}
					onclick={() => void finish()}
				>
					{busy ? t("Сохранение…") : t("Готово")}
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.wrap {
		flex: 1;
		display: grid;
		place-items: center;
		padding: var(--sp-8) var(--sp-6);
		background: var(--bg-canvas);
		background-image: var(--gradient-radial-glow);
		overflow-y: auto;
	}

	.panel {
		position: relative;
		width: 100%;
		max-width: 460px;
		padding: var(--sp-8);
		border-radius: var(--r-2xl);
		background: var(--bg-raised);
		box-shadow: var(--edge-ring), var(--edge-top), var(--shadow-overlay);
	}

	.lang {
		position: absolute;
		top: var(--sp-4);
		right: var(--sp-4);
		display: flex;
		gap: 2px;
		padding: 2px;
		border-radius: var(--r-full);
		background: var(--bg-surface);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}

	.lang-btn {
		border: 0;
		padding: 4px var(--sp-3);
		border-radius: var(--r-full);
		background: transparent;
		font-size: var(--fs-micro);
		font-weight: var(--fw-medium);
		color: var(--text-tertiary);
		cursor: pointer;
		transition:
			background var(--dur-fast) var(--ease-out),
			color var(--dur-fast) var(--ease-out);
	}
	.lang-btn:hover {
		color: var(--text-primary);
	}
	.lang-btn--on {
		background: var(--bg-active);
		color: var(--text-primary);
	}

	.steps {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		margin-bottom: var(--sp-6);
	}

	.step-dot {
		width: 7px;
		height: 7px;
		border-radius: var(--r-full);
		background: var(--bg-active);
		transition: background var(--dur-base) var(--ease-out);
	}
	.step-dot--on {
		background: var(--accent);
	}

	.step-line {
		width: 26px;
		height: 2px;
		border-radius: var(--r-full);
		background: var(--bg-active);
		transition: background var(--dur-base) var(--ease-out);
	}
	.step-line--on {
		background: var(--accent);
	}

	.mark {
		display: grid;
		place-items: center;
		width: 46px;
		height: 46px;
		margin-bottom: var(--sp-5);
		border-radius: var(--r-lg);
		background: var(--bg-surface);
		box-shadow: var(--edge-ring), var(--edge-top);
	}
	.mark img {
		width: 28px;
		height: 28px;
		object-fit: contain;
	}

	.eyebrow {
		font-size: var(--fs-micro);
		font-weight: var(--fw-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-caps);
		color: var(--text-tertiary);
	}

	.h {
		margin-top: var(--sp-2);
		font-family: var(--font-display);
		font-size: var(--fs-hero);
		font-weight: var(--fw-semibold);
		line-height: var(--lh-tight);
		letter-spacing: var(--tracking-tighter);
		color: var(--text-primary);
	}

	.lede {
		margin-top: var(--sp-3);
		font-size: var(--fs-body);
		line-height: 1.6;
		color: var(--text-secondary);
	}

	.features {
		list-style: none;
		margin: var(--sp-5) 0 var(--sp-6);
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}
	.features li {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		font-size: var(--fs-small);
		color: var(--text-secondary);
	}
	.features :global(svg) {
		color: var(--accent);
	}

	.wide {
		width: 100%;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		margin: var(--sp-5) 0;
	}

	.field-label {
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-secondary);
	}

	.hint {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.ms {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
		padding: var(--sp-3) var(--sp-4);
		border-radius: var(--r-lg);
		background: var(--bg-surface);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}

	.ms-text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.ms-title {
		font-size: var(--fs-small);
		font-weight: var(--fw-medium);
		color: var(--text-primary);
	}

	.ms-sub {
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
	}

	.ms-note {
		flex: none;
		padding: 4px var(--sp-2);
		border-radius: var(--r-sm);
		font-size: var(--fs-micro);
		color: var(--text-tertiary);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}

	.err {
		margin-top: var(--sp-4);
		padding: var(--sp-2) var(--sp-3);
		border-radius: var(--r-sm);
		font-size: var(--fs-small);
		color: var(--danger);
		background: var(--danger-soft);
	}

	.row {
		display: flex;
		justify-content: space-between;
		gap: var(--sp-3);
		margin-top: var(--sp-6);
	}
</style>
