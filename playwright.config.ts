import { defineConfig, devices } from "@playwright/test";

// E2E против vite dev с моком Tauri-бэкенда (e2e/tauri-mock.js).
// Rust-слой не участвует — он покрыт интеграционными тестами (cargo test).
export default defineConfig({
  testDir: "e2e",
  timeout: 30_000,
  use: {
    baseURL: "http://localhost:1420",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
  webServer: {
    command: "npm run dev",
    port: 1420,
    reuseExistingServer: true,
  },
});
