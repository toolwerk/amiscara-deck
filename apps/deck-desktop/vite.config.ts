import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri serves the built files; in development it loads this server.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5174, strictPort: true },
  build: { target: "es2022", outDir: "dist" },
});
