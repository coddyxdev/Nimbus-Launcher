<script lang="ts">
	import { icons, type IconName } from "$lib/icons"

	let {
		name,
		size = 16,
		strokeWidth = 1.6,
		glow = false,
	}: {
		name: IconName
		size?: number
		strokeWidth?: number
		/** Adds a soft gold drop-shadow behind the glyph, for hero/branding spots. */
		glow?: boolean
	} = $props()
</script>

<svg
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width={strokeWidth}
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	focusable="false"
	class:icon--glow={glow}
>
	<path d={icons[name]} />
</svg>

<style>
	svg {
		display: block;
		flex: none;
		transition: filter var(--dur-fast) var(--ease-out);
	}
	.icon--glow {
		filter: drop-shadow(0 0 6px var(--accent-glow));
	}
</style>
