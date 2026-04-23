import { defineWorkspace } from 'vitest/config';
import solidPlugin from 'vite-plugin-solid';

export default defineWorkspace([
  {
    plugins: [solidPlugin()],
    test: {
      name: 'lib',
      environment: 'node',
      include: ['src/lib/**/*.test.ts'],
    },
  },
  {
    plugins: [solidPlugin()],
    test: {
      name: 'components',
      environment: 'happy-dom',
      globals: true,
      setupFiles: ['./src/test-setup.ts'],
      include: ['src/components/**/*.test.{ts,tsx}'],
      transformMode: { web: [/\.[jt]sx?$/] },
    },
  },
]);
