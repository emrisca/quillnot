import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Pinned because `src-tauri/tauri.conf.json` points `devUrl` at this exact port.
// 1421 rather than Vite's default, so the dev server of another Tauri app in the
// same workspace cannot quietly win the port.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 1421,
    strictPort: true,
    // Rust writes hundreds of megabytes into `src-tauri/target`; watching it
    // makes the dev server churn for no reason.
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: { target: "es2021" },
});
