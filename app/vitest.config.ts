import { defineConfig } from "vitest/config";

// Pure-logic tests run in node; component tests (jsdom) arrive in phase 2.
export default defineConfig({ test: { environment: "node", include: ["src/**/*.test.ts"] } });

