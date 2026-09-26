import { defineConfig, devices } from '@playwright/test';
import { phoneConfig } from '@xinutec/ui-harness/config';
import harness from './e2e/harness.mjs';

/**
 * e2e for what jsdom can't see: phone-width layout, golden screenshots, and
 * whether the app loads offline. The service worker ships only in `ng build`,
 * so this runs the production build (`pnpm run e2e` builds first). Shared
 * geometry, port, server and tolerances come from @xinutec/ui-harness; this
 * app's specifics are in e2e/harness.mjs.
 */
export default defineConfig(
  // goldens: e2e/ui-golden.spec.ts keeps one committed baseline per name, with
  // no {projectName}/{platform} suffix — these only ever run on one machine (a
  // dev's Mac; CI runs Rust only, never Playwright, see .github/workflows).
  // Update them with `pnpm run ui-golden:update`.
  phoneConfig(harness, devices, { goldens: true }),
);
