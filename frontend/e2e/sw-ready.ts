import type { Page } from '@playwright/test';

/** Wait until the service worker controls the page AND has prefetched the whole
 *  shell. Going offline mid-prefetch is the race that flakes: the controller
 *  goes active first, with ~4/15 files cached. */
export async function waitForCachedShell(page: Page): Promise<void> {
  // The SW installs after the first load settles; with none this times out.
  await page.waitForFunction(() => navigator.serviceWorker?.controller != null, null, {
    timeout: 35_000,
  });
  await page.waitForFunction(
    async () => {
      const manifest: unknown = await (await fetch('/ngsw.json')).json();
      const groups =
        typeof manifest === 'object' &&
        manifest !== null &&
        'assetGroups' in manifest &&
        Array.isArray(manifest.assetGroups)
          ? (manifest.assetGroups as unknown[])
          : [];
      const app = groups.find(
        (g): g is { urls: unknown[] } =>
          typeof g === 'object' &&
          g !== null &&
          'name' in g &&
          g.name === 'app' &&
          'urls' in g &&
          Array.isArray(g.urls),
      );
      // A missing app group means the manifest is not what this test assumes.
      // `Infinity` makes the wait time out and say so; `0` would satisfy
      // `length >= want` immediately and pass without ever caching anything.
      const want = app?.urls.length ?? Infinity;
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
}
