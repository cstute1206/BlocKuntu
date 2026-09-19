import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  test: {
    environment: "jsdom",
    include: ["tests/**/*.test.ts"],
    reporters: ["default", "junit"],
    outputFile: { junit: "../Testing/results/pr-ci/gui/tests.xml" }
  }
});
