<script lang="ts">
	let {
		width = "100%",
		height = "12px",
	}: {
		width?: string
		height?: string
	} = $props()
</script>

<!-- Loading uses skeletons, never spinners. -->
<span class="skeleton" style:width style:height aria-hidden="true"></span>

<style>
	.skeleton {
		display: block;
		border-radius: var(--r-sm);
		background:
			linear-gradient(
				90deg,
				transparent 0%,
				var(--skeleton-sheen) 50%,
				transparent 100%
			)
			var(--skeleton-base);
		background-size: 220% 100%;
		animation: sheen 1300ms var(--ease-in-out) infinite;
	}

	@keyframes sheen {
		from {
			background-position: 130% 0;
		}
		to {
			background-position: -130% 0;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.skeleton {
			animation: none;
		}
	}
</style>
