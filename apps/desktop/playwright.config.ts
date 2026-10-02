import { defineConfig } from "@playwright/test"

// Browser-level layout/keyboard checks against the labeled demo entry (fixture port).
// They prove renderer behavior only — not native Tauri, keyring or WebKitGTK behavior.
export default defineConfig({
  testDir: "./e2e",
  outputDir: "./node_modules/.cache/playwright-results",
  fullyParallel: true,
  reporter: [["list"]],
  use: { baseURL: "http://localhost:3000", trace: "off" },
  webServer: {
    command: "pnpm exec vite --port 3000 --strictPort",
    url: "http://localhost:3000/demo.html",
    reuseExistingServer: true,
  },
})
