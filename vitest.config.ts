import { defineConfig } from "vitest/config";

// Юнит-тесты чистых ts-модулей (src/**/*.test.ts). E2E живут в e2e/ (Playwright)
// и vitest их не трогает.
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
