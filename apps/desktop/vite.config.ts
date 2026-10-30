import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed dev-server port and reloads the Rust side itself.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  // Loaded from disk inside the app, so one larger bundle (mostly CodeMirror) is fine.
  build: { target: "es2022", chunkSizeWarningLimit: 1000 },
});
