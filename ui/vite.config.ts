import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// During development the UI runs on Vite's server and talks to an engine
// started separately:
//   cargo run -p ols-engine -- --no-open --dev-origin http://localhost:5173
export default defineConfig({
  plugins: [react()],
  base: "./",
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/ws": { target: "ws://127.0.0.1:7420", ws: true },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
  },
});
