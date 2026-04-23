import { defineConfig } from 'vitest/config';
import solidPlugin from 'vite-plugin-solid';

// Per-test-type environments are configured in `vitest.workspace.ts`:
// - `node` for `src/lib/**/*.test.ts` (pure utilities)
// - `happy-dom` for `src/components/**/*.test.{ts,tsx}` (SolidJS components)
//
// See `specs/rules/frontend-rules.md` §1 + §14.
export default defineConfig({
  plugins: [solidPlugin()],
});
