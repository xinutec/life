import { defineConfig, devices } from '@playwright/test';
import { phoneConfig } from '@xinutec/ui-harness/config';
import harness from './e2e/harness.mjs';

/**
 * e2e harness for the things jsdom can't see — the phone-width layout checks,
 * the golden screenshots, and: does the app load OFFLINE? That last one is
 * service-worker behaviour, so everything runs against the real PRODUCTION build
 * (the SW only ships in `ng build`), served statically by the shared harness.
 * Run with `pnpm run e2e` (builds, then runs).
 *
 * Everything shared — the Pixel geometry, the port, the static server, the
 * golden tolerances — comes from @xinutec/ui-harness. What this app says about
 * itself is in e2e/harness.mjs.
 *
 * Tests live in e2e/ (outside src/), so the vitest unit runner ignores them.
 */
export default defineConfig(
  // goldens: e2e/ui-golden.spec.ts keeps one committed baseline per name, with
  // no {projectName}/{platform} suffix — these only ever run on one machine (a
  // dev's Mac; CI runs Rust only, never Playwright, see .github/workflows).
  // Update them with `pnpm run ui-golden:update`.
  phoneConfig(harness, devices, { goldens: true }),
);
