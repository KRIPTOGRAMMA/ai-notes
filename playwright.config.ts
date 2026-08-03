import { defineConfig, devices } from "@playwright/test";

// E2E against vite dev with a mocked Tauri backend (e2e/tauri-mock.js).
// The Rust layer takes no part here — it is covered by cargo test.
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
