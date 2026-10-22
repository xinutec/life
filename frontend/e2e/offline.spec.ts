import { expect, test } from '@playwright/test';

import { waitForCachedShell } from './sw-ready';

// The app is behind Nextcloud login, but the SHELL (the Angular bundle) loads
// before auth — offline it just shows the sign-in screen. We only assert the
// shell renders, so no login is needed.
test('the app shell loads offline (requires a service worker)', async ({ page, context }) => {
  await page.goto('/');
  await expect(page.locator('app-root')).not.toBeEmpty();

  // With no service worker this times out: the red that proves offline is
  // broken.
  await waitForCachedShell(page);

  // Cold-load the root with no network: the SW must serve the cached shell, not
  // the browser's offline error page.
  await context.setOffline(true);
  await page.goto('/');

  await expect(page.locator('app-root')).not.toBeEmpty();
  await expect(page.locator('body')).toContainText(/Life/i);
});
