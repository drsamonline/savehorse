import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The dev server port must match devUrl in src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2022",
    sourcemap: false,
  },
});
