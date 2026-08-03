import { defineConfig } from "vitest/config";

// Unit tests for pure ts modules (src/**/*.test.ts). E2E lives in e2e/ (Playwright)
// and vitest does not touch it.
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
