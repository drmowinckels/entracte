import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: [".vitepress/**/*.{test,spec}.ts"],
  },
});
