import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  timeout: 90_000,
  retries: 1,
  use: {
    baseURL: "http://127.0.0.1:8765",
    viewport: { width: 900, height: 700 },
    trace: "on-first-retry",
    launchOptions: {
      args: [
        "--enable-unsafe-webgpu",
        "--use-angle=swiftshader",
        "--ignore-gpu-blocklist",
      ],
    },
  },
  webServer: {
    command: "python3 -m http.server 8765",
    cwd: "../www",
    url: "http://127.0.0.1:8765",
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
