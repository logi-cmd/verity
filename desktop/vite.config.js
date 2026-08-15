import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const usePolling = process.env.VERITY_DESKTOP_USE_POLLING !== "0";

export default defineConfig({
  root: "src",
  base: "./",
  plugins: [react()],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          return id.includes("node_modules/react") || id.includes("node_modules/react-dom") ? "react-vendor" : undefined;
        }
      }
    }
  },
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: usePolling ? { usePolling: true, interval: 1000 } : undefined
  },
  preview: { host: "127.0.0.1", port: 1420, strictPort: true }
});
