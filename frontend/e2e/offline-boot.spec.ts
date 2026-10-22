import { expect, test } from '@playwright/test';

import { waitForCachedShell } from './sw-ready';

// A signed-in app opened OFFLINE must stay signed in. ngsw answers a failed
// fetch with a bodiless synthetic 504, which must not read as "logged out" —
// or the shell drops to sign-in and erases the cached identity.
test('a signed-in app opened offline stays signed in', async ({ page, context }) => {
  // Record page-context fetches so the test can prove a sync cycle really ran
  // before declaring victory — "no sign-in card yet" is vacuous otherwise.
  await page.addInitScript(() => {
    const w = window as unknown as { __fetched: string[]; fetch: typeof fetch };
    w.__fetched = [];
    const orig = w.fetch.bind(window);
    w.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
      w.__fetched.push(input instanceof Request ? input.url : String(input));
      return orig(input, init);
    };
  });

  // Phase 1 — online: harness.mjs answers /api/me, so the shell signs in (which
  // caches the identity), and the service worker installs and prefetches the
  // full shell.
  await page.goto('/');
  await expect(page.locator('ui-scaffold h1', { hasText: 'Life' })).toBeVisible();
  await waitForCachedShell(page);

  // Phase 2 — cold start with no network, on a route whose store replicates.
  await context.setOffline(true);
  await page.goto('/todo');

  // The cached identity renders the signed-in shell immediately…
  await expect(page.locator('ui-scaffold h1', { hasText: 'Life' })).toBeVisible();

  // …and it must SURVIVE replication's first cycle: wait until a sync fetch has
  // provably happened (ngsw answers it with the synthetic 504), give the signal
  // graph a beat to settle, then require the shell — not the sign-in card.
  await page.waitForFunction(
    () =>
      (window as unknown as { __fetched: string[] }).__fetched.some((u) =>
        u.includes('/api/sync/'),
      ),
    null,
    { timeout: 20_000 },
  );
  await page.waitForTimeout(1_000);

  await expect(page.locator('.signin')).toHaveCount(0);
  await expect(page.locator('ui-scaffold h1', { hasText: 'Life' })).toBeVisible();
});
