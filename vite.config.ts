import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// @tauri-apps/cli sets TAURI_DEV_HOST when running on a device/LAN
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],

  // Tauri expects a fixed port; fail if that port is not available
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
      // Don't watch the Rust backend
      ignored: ["**/src-tauri/**"],
    },
  },

  // Produce a build that Tauri can bundle
  build: {
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
  },
}));
