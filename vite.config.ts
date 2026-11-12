import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Конфигурация Vite: порт 1420 занят именно под dev-сервер Tauri,
// strictPort не даёт Vite молча уйти на соседний порт (Tauri тогда не найдёт фронтенд).
export default defineConfig({
  plugins: [react()],
  // Временные файлы esbuild держим внутри проекта, а не в системном TEMP:
  // в TEMP он не может удалить свой промежуточный файл, и сборка падает с Access is denied.
  cacheDir: "node_modules/.vite-cache",
  // Tauri ждёт статическую сборку, относительные пути безопаснее для file:// и asset-протокола
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Rust-часть пересобирает сам Tauri, Vite её не должен отслеживать
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // WebView2 в Windows 10/11 поддерживает современный синтаксис
    target: "chrome110",
    sourcemap: false,
  },
});
