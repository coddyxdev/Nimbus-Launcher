<script lang="ts">
	/**
	 * Flat front-view "paperdoll" render of a Minecraft skin texture, built
	 * from the standard 64x64 skin UV layout (head/body/arms/legs plus their
	 * hat/jacket/sleeve/pants overlay layer). No 3D involved -- just crops of
	 * the texture composited onto a canvas, pixel-for-pixel.
	 *
	 * Also understands the legacy 64x32 format (no overlay layer, no separate
	 * left arm/leg -- the right ones are mirrored into both slots, same as
	 * the game itself does for old skins).
	 */
	let {
		src,
		scale = 8,
	}: {
		/** Skin texture URL/path (already run through convertFileSrc if local). */
		src: string | null
		/** Pixels drawn per skin-texture pixel; the canvas is 16*scale by 32*scale. */
		scale?: number
	} = $props()

	const WIDTH_UNITS = 16
	const HEIGHT_UNITS = 32

	let canvas: HTMLCanvasElement | undefined = $state()
	let failed = $state(false)

	/** [sx, sy, sw, sh, dx, dy] -- source rect in texture pixels, destination in paperdoll units. */
	type Blit = [number, number, number, number, number, number]

	function paint(img: HTMLImageElement) {
		if (!canvas) return
		const ctx = canvas.getContext("2d")
		if (!ctx) return

		canvas.width = WIDTH_UNITS * scale
		canvas.height = HEIGHT_UNITS * scale
		ctx.imageSmoothingEnabled = false
		ctx.clearRect(0, 0, canvas.width, canvas.height)

		const blit = ([sx, sy, sw, sh, dx, dy]: Blit) => {
			ctx.drawImage(img, sx, sy, sw, sh, dx * scale, dy * scale, sw * scale, sh * scale)
		}

		// Base layer -- front faces only, laid out as a flat 16x32 paperdoll:
		// left-arm-column(4) + body-column(8) + right-arm-column(4) wide,
		// head(8) + torso/arms(12) + legs(12) tall.
		blit([8, 8, 8, 8, 4, 0]) // head
		blit([20, 20, 8, 12, 4, 8]) // body
		blit([44, 20, 4, 12, 0, 8]) // right arm -> viewer's left
		blit([4, 20, 4, 12, 4, 20]) // right leg -> viewer's left

		const legacy = img.naturalHeight <= 32
		if (legacy) {
			// Pre-1.8 skins have no separate left limbs -- mirror the right ones.
			blit([44, 20, 4, 12, 12, 8])
			blit([4, 20, 4, 12, 8, 20])
			return
		}

		blit([36, 52, 4, 12, 12, 8]) // left arm -> viewer's right
		blit([20, 52, 4, 12, 8, 20]) // left leg -> viewer's right

		// Overlay layer (hat / jacket / sleeves / pant legs). Transparent
		// pixels let the base layer underneath show through untouched.
		blit([40, 8, 8, 8, 4, 0]) // hat
		blit([20, 36, 8, 12, 4, 8]) // jacket
		blit([44, 36, 4, 12, 0, 8]) // right sleeve
		blit([52, 52, 4, 12, 12, 8]) // left sleeve
		blit([4, 36, 4, 12, 4, 20]) // right pant leg
		blit([4, 52, 4, 12, 8, 20]) // left pant leg
	}

	function clear() {
		if (!canvas) return
		canvas.width = WIDTH_UNITS * scale
		canvas.height = HEIGHT_UNITS * scale
		canvas.getContext("2d")?.clearRect(0, 0, canvas.width, canvas.height)
	}

	$effect(() => {
		failed = false
		if (!src) {
			clear()
			return
		}
		// Cache-bust: the same path can point at a file whose bytes just
		// changed (a new skin overwriting the old one under the same name).
		const bust = `${src}${src.includes("?") ? "&" : "?"}t=${Date.now()}`
		const img = new Image()
		img.onload = () => paint(img)
		img.onerror = () => (failed = true)
		img.src = bust
	})
</script>

<div
	class="skin-preview"
	style={`width:${WIDTH_UNITS * scale}px;height:${HEIGHT_UNITS * scale}px;`}
>
	{#if failed}
		<span class="skin-preview-fallback" aria-hidden="true">?</span>
	{:else}
		<canvas bind:this={canvas}></canvas>
	{/if}
</div>

<style>
	.skin-preview {
		position: relative;
		flex: none;
		display: grid;
		place-items: center;
		border-radius: var(--r-md);
		background: var(--bg-inset);
		box-shadow: var(--edge-ring);
		overflow: hidden;
	}

	canvas {
		display: block;
		image-rendering: pixelated;
	}

	.skin-preview-fallback {
		font-size: var(--fs-title);
		color: var(--text-disabled);
	}
</style>
