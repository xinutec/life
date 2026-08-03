import { expect, test } from '@playwright/test';

// Beyond the shell loading offline, the read data (inventory etc.) must be
// readable offline too — the Tube case. ngsw dataGroups cache the API responses
// network-first; offline they come from cache. (serve.mjs mocks /api/items.)
test('cached API data is readable offline', async ({ page, context }) => {
  await page.goto('/');
  await page.waitForFunction(() => navigator.serviceWorker?.controller != null, null, {
    timeout: 35_000,
  });

  // Hit /api/items online so the SW caches the response.
  // `Response.json()` is `any` by the DOM lib's design, so the shape is named
  // here rather than inherited. The test only cares that it is a non-empty list
  // and that the offline read matches it, so `unknown[]` is the whole contract.
  const online = await page.evaluate<unknown[]>(() =>
    fetch('/api/items').then((r) => r.json()),
  );
  expect(online.length).toBeGreaterThan(0);

  // Wait until it lands in a data cache (dataGroups live in :data:* caches).
  await page.waitForFunction(
    async () => {
      for (const k of await caches.keys()) {
        if (k.includes(':data') && (await (await caches.open(k)).match('/api/items'))) return true;
      }
      return false;
    },
    null,
    { timeout: 20_000 },
  );

  // Offline: the same data must be served from cache, not fail.
  await context.setOffline(true);
  const offline = await page.evaluate<unknown[] | null>(() =>
    fetch('/api/items')
      .then((r) => r.json())
      .catch(() => null),
  );
  expect(offline).toEqual(online);
});
