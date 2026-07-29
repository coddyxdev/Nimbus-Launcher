import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],

  // Mirrors compilerOptions.paths in tsconfig.json. Keep the two in sync:
  // svelte-check reads tsconfig, Rollup reads this block.
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
      $components: fileURLToPath(new URL("./src/components", import.meta.url)),
    },
  },

  // Tauri expects a fixed port and fails fast instead of silently moving.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },

  // outDir MUST match `build.frontendDist` in src-tauri/tauri.conf.json,
  // otherwise the release binary falls back to devUrl and shows a
  // WebView connection error instead of the launcher.
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "chrome110",
    minify: "esbuild",
    sourcemap: false,
  },
});
