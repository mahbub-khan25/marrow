import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    root: "ui",
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
