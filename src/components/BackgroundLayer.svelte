<script lang="ts">
	/**
	 * The user's picture or clip, painted behind the entire launcher.
	 *
	 * It sits in its own fixed layer rather than as a CSS background on the
	 * shell: a <video> cannot be a CSS background, and keeping both media types
	 * in one layer means opacity, blur and the readability scrim behave
	 * identically whether the user picked a JPG or an MP4.
	 *
	 * The layer is inert - no pointer events, no tab stop, hidden from screen
	 * readers - so it can never intercept a click meant for the UI.
	 */
	import { background } from "$lib/background.svelte"

	// Remounts the element when the file changes, so the WebView never keeps
	// showing the previous picture from its cache.
	const src = $derived(background.src)
	const kind = $derived(background.kind)
</script>

{#if src}
	<div class="bg" aria-hidden="true">
		{#key src}
			{#if kind === "video"}
				<!-- svelte-ignore a11y_media_has_caption -->
				<video class="media" {src} autoplay loop muted playsinline disablepictureinpicture></video>
			{:else}
				<img class="media" {src} alt="" draggable="false" />
			{/if}
		{/key}

	</div>
{/if}

<style>
	.bg {
		position: fixed;
		inset: 0;
		z-index: 0;
		overflow: hidden;
		pointer-events: none;
		user-select: none;
		/* Matches the themed canvas, so a picture with transparency or a
		   letterboxed clip blends into the theme instead of showing black. */
		background: var(--bg-canvas);
	}

	.media {
		width: 100%;
		height: 100%;
		object-fit: cover;
		opacity: var(--bg-media-opacity, 0.55);
		/* Both default to no-ops so an unblurred picture is never resampled:
		   no filter layer, no upscale, native pixels straight to the screen. */
		filter: var(--bg-media-filter, none);
		transform: scale(var(--bg-media-scale, 1));
		/* Keeps the browser on its best downscaling path for large photos. */
		image-rendering: auto;
		transition: opacity var(--dur-base) var(--ease-out);
	}

	@media (prefers-reduced-motion: reduce) {
		.media {
			transition: none;
		}
	}
</style>
