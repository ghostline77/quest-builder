import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.spec.ts",
  fullyParallel: false,
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:8891",
    viewport: { width: 1440, height: 1000 },
  },
  webServer: {
    command:
      "npm exec vite -- preview --host 127.0.0.1 --port 8891 --strictPort",
    url: "http://127.0.0.1:8891",
    reuseExistingServer: false,
  },
});
