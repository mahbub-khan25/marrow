import { defineConfig } from "vite";

// The UI source lives in ui/; Tauri loads the built files from dist/.
export default defineConfig({
  root: "ui",
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2021",
    chunkSizeWarningLimit: 1500,
  },
});
