import { expect, test } from '@playwright/test';

// The regression this pins (2026-07-16): opening the app OFFLINE dumped a
// signed-in user onto the sign-in screen — and erased the cached identity, so
// every later offline launch went straight to login too. The chain: ngsw
// intercepts every fetch and answers network failure with a bodiless synthetic
// 504 (it never lets the fetch reject); the sync auth guard read any non-JSON
// response as "logged out"; AuthState.lost then wiped `me` AND the localStorage
// cache. offline.spec.ts couldn't see this — it never signs in, so it happily
// asserted the sign-in screen renders offline. This test holds the invariant
// that actually matters: a signed-in app opened offline STAYS signed in.
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

  // Phase 1 — online: serve.mjs answers /api/me, so the shell signs in (which
  // caches the identity), and the service worker installs and prefetches the
  // full shell (same completeness wait as offline.spec.ts — going offline
  // mid-prefetch is a flake).
  await page.goto('/');
  await expect(page.locator('mat-toolbar .brand')).toBeVisible();
  await page.waitForFunction(() => navigator.serviceWorker?.controller != null, null, {
    timeout: 35_000,
  });
  await page.waitForFunction(
    async () => {
      const manifest = await (await fetch('/ngsw.json')).json();
      const want = manifest.assetGroups.find((g: { name: string }) => g.name === 'app').urls.length;
      for (const key of await caches.keys()) {
        if (key.includes('assets:app:cache')) {
          return (await (await caches.open(key)).keys()).length >= want;
        }
      }
      return false;
    },
    null,
    { timeout: 60_000 },
  );

  // Phase 2 — cold start with no network, on a route whose store replicates.
  await context.setOffline(true);
  await page.goto('/todo');

  // The cached identity renders the signed-in shell immediately…
  await expect(page.locator('mat-toolbar .brand')).toBeVisible();

  // …and it must SURVIVE replication's first cycle: wait until a sync fetch has
  // provably happened (ngsw answers it with the synthetic 504), give the signal
  // graph a beat to settle, then require the shell — not the sign-in card.
  await page.waitForFunction(
    () => (window as unknown as { __fetched: string[] }).__fetched.some((u) => u.includes('/api/sync/')),
    null,
    { timeout: 20_000 },
  );
  await page.waitForTimeout(1_000);

  await expect(page.locator('.signin')).toHaveCount(0);
  await expect(page.locator('mat-toolbar .brand')).toBeVisible();
});
