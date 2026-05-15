import { defineConfig } from "vite";

// Vite config tuned for Tauri dev. The Tauri CLI sets TAURI_DEV_HOST when
// running on a remote host (e.g. iOS device); otherwise we serve from
// localhost. The fixed port + strictPort pair guarantees Tauri's webview
// always finds the dev server at the URL declared in tauri.conf.json.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  // Prevent vite from clearing the screen so we keep tauri's logs visible.
  clearScreen: false,

  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Ignore the parent Rust crate while developing the TS frontend.
      ignored: ["**/src-tauri/**", "**/target/**"],
    },
  },

  // The Tauri webview only ships modern Chromium / WebKit; we can target ESNext.
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "esnext",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
