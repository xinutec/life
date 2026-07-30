// The app-specific half of the shared phone-width harness (@xinutec/ui-harness).
// Read by BOTH playwright.config.ts and the harness's static server, so there is
// one place to say what this app is and no port to keep in step — the port is
// allocated from `app`.

/** @type {import('@xinutec/ui-harness/config').HarnessSpec} */
export default {
  app: 'life',
  dist: 'dist/life-web/browser',
  // A tiny mock of the read API, so the offline-data e2e can prove that
  // responses are cached and served with no network. Real prod is the Rust
  // backend.
  api: {
    '/api/me': { userId: 'test', displayName: 'Test', avatarUrl: '', nextcloud: 'not_linked' },
    '/api/items': [
      {
        id: 1, product_id: null, name: 'Cached Avocado', brand: null, category: 'food',
        quantity: null, unit: null, expiry: null, location_id: null, barcode: null,
        has_image: false,
      },
    ],
  },
};
