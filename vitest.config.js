import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/unit/**/*.test.js', 'tests/integration/**/*.test.js', 'tests/e2e/**/*.test.js'],
    exclude: ['node_modules', 'dist'],
    globals: false,
    testTimeout: 10000,
  },
});
