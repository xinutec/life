import { test, expect, type Page } from '@playwright/test';
// The fleet-shared harness, published as @xinutec/ui-harness (source repo
// ~/Code/ui-harness). Ships compiled JS, so it loads straight from node_modules.
import {
  expectNoTextOverlaps,
  expectNoHorizontalOverflow,
  expectNoClippedText,
  expectViewportIsPhone,
} from '@xinutec/ui-harness';

/**
 * UI-measurement checks (ported from the health-sync frontend): render the
 * main screens at a phone viewport (Pixel 9, 412px — see playwright.config)
 * with the backend mocked and busy data, and assert the two layout failure
 * classes that read fine in source and only show on a real phone:
 *   1. no two pieces of rendered text collide (the to-do rows' pills crowding
 *      the title were exactly this), and
 *   2. nothing spills past the right edge (a bottom sheet's toggle-groups are
 *      the classic culprit).
 *
 * The service worker is blocked: SW-controlled fetches bypass page.route,
 * and these tests are about layout, not offline (e2e/offline*.spec.ts).
 */
test.use({ serviceWorkers: 'block' });

const iso = (daysFromNow: number): string => {
  const d = new Date();
  d.setDate(d.getDate() + daysFromNow);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};

const ME = { userId: 'test', displayName: 'Test User', avatarUrl: '', nextcloud: 'active' };

/** Busy to-do set: overdue+high (two pills + note), due-soon, ready, waiting. */
const TODOS = [
  { ulid: '01TODOOVERDUE0000000000001', id: 1, title: 'Call the GP about the referral letter',
    type: 'call', status: 'open', priority: 'high', notes: 'ask for the clinic line — they only pick up mornings',
    notBefore: null, due: iso(-3), rev: 1, _deleted: false },
  { ulid: '01TODODUESOON0000000000002', id: 2, title: 'Renew the travel insurance policy',
    type: 'admin', status: 'open', priority: 'medium', notes: null,
    notBefore: null, due: iso(2), rev: 2, _deleted: false },
  { ulid: '01TODOPLAIN000000000000003', id: 3, title: 'Descale the coffee machine',
    type: 'task', status: 'open', priority: null, notes: 'vinegar under the sink',
    notBefore: null, due: null, rev: 3, _deleted: false },
  { ulid: '01TODOWAITING0000000000004', id: 4, title: 'Book the summer service',
    type: 'appointment', status: 'open', priority: 'low', notes: null,
    notBefore: iso(10), due: null, rev: 4, _deleted: false },
];

const SHOPPING = [
  { ulid: '01SHOPA0000000000000000001', id: 1, name: 'Greek yoghurt (the big tubs)',
    quantity: 2, unit: 'tubs', barcode: null, done: false, rev: 1, _deleted: false },
  { ulid: '01SHOPB0000000000000000002', id: 2, name: 'Kidney beans', quantity: 3,
    unit: 'tins', barcode: null, done: true, rev: 2, _deleted: false },
];

const now = new Date();
const at = (daysAgo: number, h: number): string => {
  const d = new Date(now);
  d.setDate(d.getDate() - daysAgo);
  d.setHours(h, 24, 0, 0);
  return d.toISOString();
};
// The trend chart's window rolls back from "now", so today's entries must be
// unambiguously in the past — a fixed clock hour would sit in the future when
// the suite runs earlier in the day and drop out of the window.
const hoursAgo = (n: number): string => new Date(now.getTime() - n * 3_600_000).toISOString();
const WELLBEING = [
  { ulid: '01WELLA0000000000000000001', id: 1, recordedAt: hoursAgo(5), scoreTenths: 20, energyTenths: 20,
    emotions: ['Anxious', 'Withdrawn'], note: 'rough morning', rev: 1, _deleted: false },
  { ulid: '01WELLB0000000000000000002', id: 2, recordedAt: hoursAgo(1), scoreTenths: 40, energyTenths: null,
    emotions: [], note: null, rev: 2, _deleted: false },
  { ulid: '01WELLC0000000000000000003', id: 3, recordedAt: at(1, 20), scoreTenths: 35, energyTenths: null,
    emotions: [], note: null, rev: 3, _deleted: false },
];

const ITEMS = [
  { id: 1, product_id: null, name: 'Milk (semi-skimmed)', brand: 'Waitrose Essential', category: 'food',
    quantity: 1, unit: 'bottle', expiry: iso(-1), location_id: 2, barcode: null, has_image: false },
  { id: 2, product_id: null, name: 'Chicken thighs', brand: null, category: 'food',
    quantity: 500, unit: 'g', expiry: iso(1), location_id: 2, barcode: null, has_image: false },
];

const LOCATIONS = [
  { id: 1, kind: 'room', name: 'Kitchen', parent_id: null, sort_order: 0, position: null },
  { id: 2, kind: 'fridge', name: 'Fridge', parent_id: 1, sort_order: 0, position: null },
  { id: 3, kind: 'cupboard', name: 'Spice cupboard', parent_id: 1, sort_order: 1, position: null },
];

const RECIPES = [
  { id: 1, name: 'Chicken curry', instructions: null, servings: 4, ingredients: [
    { name: 'Chicken thighs', quantity: 500, unit: 'g' },
    { name: 'Coconut milk', quantity: 1, unit: 'tin' },
    { name: 'Curry paste', quantity: 2, unit: 'tbsp' },
  ] },
  { id: 2, name: 'Beans on toast', instructions: null, servings: 1, ingredients: [
    { name: 'Kidney beans', quantity: 1, unit: 'tin' },
    { name: 'Bread', quantity: 2, unit: 'slices' },
  ] },
];

const TRASH = [
  { kind: 'shopping', ref: '01TRASHSHOP000000000000001', name: 'Oat milk (the barista one)',
    deleted_at: now.getTime() - 3_600_000 },
  { kind: 'recipe', ref: '3', name: 'Lentil soup', deleted_at: now.getTime() - 86_400_000 },
];

/** A fully-populated product detail: two shops with prices + deep links, the
 *  whole nutrition panel, long ingredients, allergen + dietary chips — the
 *  busiest the product page gets. */
const PRODUCT_DETAIL = {
  product: { id: 42, barcode: '5000328042732', name: 'Quaker Oat So Simple Original Big Pack Porridge Sachets',
    brand: 'Quaker', quantity_label: '22x27G', source: 'off', external_id: '5000328042732',
    name_source: 'asda', has_image: false },
  listings: [
    { source: 'off', external_id: '5000328042732',
      url: 'https://world.openfoodfacts.org/product/5000328042732', raw_name: 'oat so simple' },
    { source: 'asda', external_id: '9346702',
      url: 'https://www.asda.com/groceries/product/9346702', raw_name: 'Quaker Oat So Simple' },
    { source: 'waitrose', external_id: '271105',
      url: 'https://www.waitrose.com/ecom/products/x/271105', raw_name: 'Oat So Simple' },
  ],
  prices: [
    { source: 'waitrose', external_id: '271105', amount_minor: 450, currency: 'GBP', unit_amount_minor: null,
      unit_measure: null, region: null, observed_at: now.getTime() },
    { source: 'asda', external_id: '9346702', amount_minor: 475, currency: 'GBP', unit_amount_minor: 800,
      unit_measure: 'KG', region: 'EN', observed_at: now.getTime() - 2 * 86_400_000 },
  ],
  facts: {
    nutrition: { basis: '100g', serving_size: '40 g', energy_kj: 1500, energy_kcal: 356,
      fat_g: 6.5, saturates_g: 1.2, carbohydrate_g: 60, sugars_g: 1, fibre_g: 10,
      protein_g: 11, salt_g: 0.1, extra: { sodium: 0.04 } },
    ingredients:
      'Wholegrain rolled oats (95%), sugar, natural flavouring, salt, an improbably ' +
      'long tail of emulsifiers and stabilisers to make this paragraph wrap on a phone',
    allergens: [
      { allergen: 'gluten', presence: 'contains' },
      { allergen: 'milk', presence: 'may_contain' },
      { allergen: 'nuts', presence: 'may_contain' },
    ],
    dietary: [
      { flag: 'gluten_free', value: 'no' },
      { flag: 'organic', value: 'yes' },
      { flag: 'palm_oil_free', value: 'maybe' },
      { flag: 'vegan', value: 'yes' },
      { flag: 'vegetarian', value: 'yes' },
    ],
  },
  reconciliation: { fields: [] },
  documents: [],
};

/** A product Open Food Facts knows under a cryptic crowd name, that no shop
 *  lists yet — the state the "Find at Asda" lookup exists for. */
const UNLISTED_DETAIL = {
  product: { id: 43, barcode: '5063089281581', name: 'Asda ES Balsamic Modena', brand: 'Asda',
    quantity_label: null, source: 'off', external_id: '5063089281581', name_source: 'off',
    has_image: false },
  listings: [
    { source: 'off', external_id: '5063089281581',
      url: 'https://world.openfoodfacts.org/product/5063089281581', raw_name: 'Asda ES Balsamic Modena' },
  ],
  prices: [],
  facts: { nutrition: null, ingredients: null, allergens: [], dietary: [] },
  reconciliation: { fields: [] },
  documents: [],
};

/** Asda's real answer for that crowd name: the product itself ranks LAST,
 *  behind a raspberry glaze. The barcode is what identifies it. */
const ASDA_HITS = [
  { external_id: '2266257', name: 'Glaze with Balsamic Vinegar of Modena 250ml', brand: 'Asda',
    barcode: '5050854946264', quantity_label: '250ml', price_label: '£2.25', price: null, image_url: null },
  { external_id: '9020293', name: 'Raspberry Glaze with Balsamic Vinegar of Modena', brand: 'Asda',
    barcode: '5063089281598', quantity_label: '250ml', price_label: '£2.50', price: null, image_url: null },
  { external_id: '9020290', name: 'Extra Special Balsamic Vinegar of Modena 250ml', brand: 'Asda',
    barcode: '5063089281581', quantity_label: '250ml', price_label: '£8.00',
    price: { amount_minor: 800, currency: 'GBP', unit_amount_minor: null, unit_measure: null, region: 'EN' },
    image_url: null },
];

const CONFLICTS = [
  { id: 1, kind: 'todo', ulid: '01TODOOVERDUE0000000000001', field: 'title',
    label: 'Call the GP about the referral letter',
    mine: JSON.stringify('Call the GP about the referral letter'),
    theirs: JSON.stringify('Phone the GP re: the referral'),
    created_at: now.getTime() - 60_000 },
];

/** Mock every backend call: pulls return the seed docs, pushes accept all.
 *  Catch-all FIRST — Playwright runs handlers last-registered-first. */
async function mockApi(page: Page): Promise<void> {
  await page.route('**/api/**', (r) =>
    r.request().method() === 'GET' ? r.fulfill({ json: [] }) : r.fulfill({ status: 204, body: '' }),
  );
  await page.route('**/api/me', (r) => r.fulfill({ json: ME }));
  await page.route('**/api/items*', (r) => r.fulfill({ json: ITEMS }));
  await page.route('**/api/locations*', (r) => r.fulfill({ json: LOCATIONS }));
  await page.route('**/api/recipes', (r) => r.fulfill({ json: RECIPES }));
  await page.route('**/api/cookable*', (r) => r.fulfill({ json: [RECIPES[1]] }));
  await page.route('**/api/trash*', (r) => r.fulfill({ json: TRASH }));
  await page.route('**/api/products/id/42', (r) => r.fulfill({ json: PRODUCT_DETAIL }));
  await page.route('**/api/products/id/43', (r) => r.fulfill({ json: UNLISTED_DETAIL }));
  // The picker's Asda tier still searches by name.
  await page.route('**/api/products/shop/asda*', (r) => r.fulfill({ json: ASDA_HITS }));
  // The product page's lookup is answered server-side: the backend checks what
  // past shop queries taught it before searching, and matches on the EAN itself
  // (products::asda::match_barcode). So the wire carries an already-confirmed
  // hit, and what's exercised here is how that answer renders.
  await page.route('**/api/products/id/43/find/asda', (r) =>
    r.fulfill({ json: { hit: ASDA_HITS[2], from_cache: false } }),
  );
  await page.route('**/api/conflicts*', (r) => r.fulfill({ json: CONFLICTS }));
  const sync = (docs: unknown[]) => (r: Parameters<Parameters<Page['route']>[1]>[0]) => {
    if (r.request().method() === 'POST') return r.fulfill({ json: [] });
    const since = Number(new URL(r.request().url()).searchParams.get('since') ?? '0');
    // Incremental protocol: only send the seed once, else the pull loops forever.
    const fresh = docs.filter((d) => (d as { rev: number }).rev > since);
    const top = docs.reduce((m, d) => Math.max(m, (d as { rev: number }).rev), since);
    return r.fulfill({ json: { documents: fresh, checkpoint: { rev: top } } });
  };
  await page.route('**/api/sync/todo?*', sync(TODOS));
  await page.route('**/api/sync/todo', sync(TODOS));
  await page.route('**/api/sync/todo_link*', sync([]));
  await page.route('**/api/sync/shopping*', sync(SHOPPING));
  await page.route('**/api/sync/wellbeing*', sync(WELLBEING));
}

// The checker-checker: this suite once ran at 1280×720 for months while its
// titles said "phone width" (a device spread overrode the viewport). If
// emulation ever silently drops again, fail HERE, loudly.
test('the suite really runs at phone geometry', async ({ page }) => {
  await mockApi(page);
  await page.goto('/today');
  await expectViewportIsPhone(page);
});

test('today — busy composition: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/today');
  await page.getByText('Needs you').waitFor();
  await page.getByText('Call the GP', { exact: false }).waitFor();
  await page.getByText('Expiring soon').waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('to-do list — pills in rows: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/todo');
  await page.getByText('Call the GP', { exact: false }).waitFor();
  await page.getByText('overdue', { exact: false }).first().waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('wellbeing — chart + timeline: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.getByText('How do you feel right now?').waitFor();
  await page.getByText('Mood · last 7 days').waitFor();
  await page.getByText('Energy · last 7 days').waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('wellbeing — the two charts agree on where the days are', async ({ page }) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.getByText('Energy · last 7 days').waitFor();
  // Mood and energy share one x axis (the same window, the same instant), so a
  // midnight must land on the same pixel in both — otherwise the day rules stagger
  // down the page and the charts can't be read against each other. Measure the RULES
  // themselves, not the svg boxes: the boxes matched even when the axis words had
  // collapsed to nothing, which is exactly how this test missed a broken chart once.
  const rules = (chart: number) =>
    page
      .locator('svg.chart')
      .nth(chart)
      .locator('line.day')
      .evaluateAll((els) => els.map((e) => e.getBoundingClientRect().x));
  const [mood, energy] = [await rules(0), await rules(1)];
  expect(mood.length).toBeGreaterThan(0);
  expect(energy.length).toBe(mood.length);
  for (let i = 0; i < mood.length; i++) expect(Math.abs(mood[i] - energy[i])).toBeLessThan(0.5);
});

test('wellbeing — a half-step reads as one feeling between two faces', async ({ page }) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.getByText('Mood · last 7 days').waitFor();

  // The seeded 3.5 (35 tenths). Its chip says so, rather than rounding to a 3 or a
  // 4 — the whole point of recording "4, but a bit lower at the gym".
  const half = page.locator('.entry[data-score="35"]');
  await expect(half).toHaveAttribute('aria-label', /okay–good/);

  // Open it: BOTH faces of the half-step light up, and neither at full strength —
  // two full faces would say "I felt two things", which is what a half-step avoids.
  await half.click();
  const faces = page.locator('.sheet-form .faces').first().locator('.face');
  await expect(faces.nth(2)).toHaveClass(/\bon\b/); // okay
  await expect(faces.nth(3)).toHaveClass(/\bon\b/); // good
  await expect(faces.nth(2)).toHaveClass(/\bhalf\b/);
  await expect(faces.nth(4)).not.toHaveClass(/\bon\b/); // great is not part of it
  await expect(page.locator('.reading')).toContainText('3.5/5');
});

test('wellbeing — the axis words are actually on the screen', async ({ page }) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.getByText('Energy · last 7 days').waitFor();
  // The check the others were all missing: can he READ them? Absolutely-positioned
  // axis words once collapsed their own column to zero width and slid off the left
  // edge of the phone — while a same-x/same-y test, a vertical-alignment test and
  // the shared overflow harness (which only measures the RIGHT edge) all passed.
  // Count first: "no word is off-screen" is also true when there are no words, and
  // a vacuous pass is how the last three tests missed a chart he couldn't read.
  const words = page.locator('svg.chart text.axis-word');
  await expect(words).toHaveCount(6); // three on each of the two charts
  const offscreen = await words.evaluateAll((els) =>
    els
      .filter((e) => e.getBoundingClientRect().left < 0)
      .map((e) => `${e.textContent?.trim()} @ ${Math.round(e.getBoundingClientRect().left)}px`),
  );
  expect(offscreen).toEqual([]);
});

test('wellbeing — each axis word sits level with the dot it names', async ({ page }) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.getByText('Energy · last 7 days').waitFor();
  // "great"/"okay"/"awful" claim to name the 5, the 3 and the 1, so each must sit at
  // the height that reading actually plots at. Spaced evenly down a CSS column
  // instead (the obvious way, and what this used to do) "awful" landed 14px above
  // where a 1 plots. The y here is the plot's own: viewBox 0 0 300 96, padTop 8,
  // padBottom 18 — so a 5, a 3 and a 1 plot at 8, 44 and 78.
  const svg = (await page.locator('svg.chart').first().boundingBox())!;
  const scale = svg.height / 96;
  const levels = [8, 44, 78].map((u) => svg.y + u * scale);
  const words = page.locator('svg.chart').first().locator('text.axis-word');
  await expect(words).toHaveCount(3);
  for (let i = 0; i < 3; i++) {
    const b = (await words.nth(i).boundingBox())!;
    expect(Math.abs(b.y + b.height / 2 - levels[i])).toBeLessThan(1.5);
  }
});

test('buy — list + bought bar: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/shopping');
  await page.getByText('Greek yoghurt', { exact: false }).waitFor();
  await page.getByText('add to inventory', { exact: false }).waitFor();
  // Mid-shop (an item is checked, so the bought bar is up) the add-FAB must
  // still be there — capture can't go missing for most of a real shop.
  await expect(page.getByRole('button', { name: 'Add to the list' })).toBeVisible();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('settings — about card: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Check for updates' }).waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('inventory — items + places: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/inventory');
  await page.getByText('Milk (semi-skimmed)').waitFor();
  await page.getByText('Kitchen › Fridge', { exact: true }).waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

// The product picker — successor to the Find-on-Waitrose dialog this oracle was
// built for, whose outline "Search" label was sheared in half by
// mat-dialog-content's zeroed top padding; nothing caught it until it shipped.
// Open it and assert no text is clipped. The shop bridge is Android-only, so
// stub it so the shop tier renders too.
test('product-picker dialog — the Search label is not sheared @ phone width', async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    (window as unknown as { ShopBridge: unknown }).ShopBridge = {
      available: () => true,
      connect: () => {},
      run: () => {},
    };
  });
  await mockApi(page);
  await page.goto('/inventory');
  await page.getByText('Milk (semi-skimmed)').click();
  await page.getByRole('button', { name: 'Find a product' }).click();

  const dialog = page.locator('.mat-mdc-dialog-container');
  await dialog.waitFor();
  // The outline field (with the "Search" label under test) is what must be
  // rendered before we measure; the shop tier's button proves that section
  // rendered too.
  await dialog.locator('mat-form-field').waitFor();
  await dialog.getByRole('button', { name: /Search Waitrose/ }).waitFor();

  await expectNoClippedText(page, testInfo, '.mat-mdc-dialog-container');
  await expectNoTextOverlaps(page, testInfo, '.mat-mdc-dialog-container');
  await expectNoHorizontalOverflow(page, testInfo, '.mat-mdc-dialog-container');
});

test('all items — filter + brand + expiry rows: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/items');
  await page.getByText('Chicken thighs').waitFor();
  await page.getByText('Waitrose Essential', { exact: false }).waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('recipes — ingredient-chip cards: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/recipes');
  await page.getByText('Chicken curry').waitFor();
  await page.getByText('cookable with what', { exact: false }).waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('product page — prices, panel, chips: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/product/42');
  await page.getByText('Quaker Oat So Simple', { exact: false }).first().waitFor();
  await page.getByText('Where to buy').waitFor();
  await page.getByText('£4.50').waitFor();
  await page.getByText('Nutrition', { exact: false }).waitFor();
  await page.getByText('of which saturates').waitFor();
  await page.getByText('may contain milk').waitFor();
  await page.getByText('Open Food Facts').waitFor();
  // Scoped to the page's own text: this screen is the first that genuinely
  // outgrows a phone viewport, and mid-scroll its content passes BEHIND the
  // fixed bottom nav (opaque by design). A whole-page assertion would read that
  // as a collision. Overflow stays whole-page — that's the body scroller's job.
  await expectNoTextOverlaps(page, testInfo, 'app-product-page');
  await expectNoHorizontalOverflow(page, testInfo);
});

test('product page — the Asda match reads cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/product/43');
  await page.getByRole('button', { name: 'Find at Asda' }).click();
  // The confirmed match — the product itself, which Asda's own relevance order
  // ranks last. That rule is enforced (and tested) server-side now.
  await page.getByText('Extra Special Balsamic Vinegar of Modena').waitFor();
  await page.getByText('same barcode', { exact: false }).waitFor();
  await expectNoTextOverlaps(page, testInfo, 'app-product-page');
  await expectNoHorizontalOverflow(page, testInfo);
});

test('product page — the reconcile panel lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  // A product whose sources disagree — a long product name is the worst case for
  // the stacked radio options.
  await page.route('**/api/products/id/44', (r) =>
    r.fulfill({
      json: {
        ...UNLISTED_DETAIL,
        product: { ...UNLISTED_DETAIL.product, id: 44 },
        reconciliation: {
          fields: [
            {
              field: 'name',
              label: 'Name',
              current: 'Asda ES Balsamic Modena',
              candidates: [
                { source: 'asda', value: 'Extra Special Balsamic Vinegar of Modena 250ml' },
              ],
            },
            {
              field: 'quantity_label',
              label: 'Pack size',
              current: '250ML',
              candidates: [{ source: 'asda', value: '250ml' }],
            },
          ],
        },
      },
    }),
  );
  await page.goto('/product/44');
  await page.getByText('Shops disagree on some details').waitFor();
  await page.getByText('Pack size').waitFor();
  await page.getByRole('button', { name: 'Apply' }).waitFor();
  await expectNoTextOverlaps(page, testInfo, 'app-product-page');
  await expectNoHorizontalOverflow(page, testInfo);
});

test('trash — restorable rows: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/trash');
  await page.getByText('Oat milk', { exact: false }).waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('conflicts — kept/theirs cards: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/conflicts');
  await page.getByText('Kept (this device)').waitFor();
  await page.getByRole('button', { name: 'Use theirs' }).waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

// The emotion picker is the layout class that breaks silently: a full-screen
// dialog with a sticky header (search + Done) over the whole vocabulary — every
// family, every word, on one surface. Nothing is hidden behind an accordion, so
// a word that wraps badly or spills sideways has nowhere to hide either.
test('emotion picker — full mosaic + sticky header: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  // Open the seeded morning check-in (the score-2 entry — it has emotions),
  // then the picker via the sheet's Add-emotions button.
  await page.locator('.entry[data-score="20"]').click();
  await page.locator('button.add-emotions').click();
  const picker = page.locator('.picker');
  await picker.waitFor();
  // Not a substring match: "Add Abandoned" contains "Done".
  await page.getByRole('button', { name: /^Done/ }).waitFor();
  // Every family is on screen at once — no expanding needed. Words from the first
  // and last families must both be present.
  await expect(page.locator('.family')).toHaveCount(7);
  await page.getByRole('button', { name: 'Add Curious' }).waitFor();
  await page.getByRole('button', { name: 'Add Energetic' }).waitFor();
  // The selected-set footer is opaque and sticky; vocabulary text scrolling
  // behind it is occluded, not colliding — the same false-positive the to-do
  // sheet test scopes around. Measure each pinned region and the body apart.
  await expectNoTextOverlaps(page, testInfo, '.picker .top');
  await expectNoTextOverlaps(page, testInfo, '.picker .body');
  await expectNoTextOverlaps(page, testInfo, '.picker .selected');
  await expectNoHorizontalOverflow(page, testInfo, '.picker');
  await expectNoHorizontalOverflow(page, testInfo);
  // The picker isn't a form-dialog (custom full-screen chrome, not app-dialog),
  // but its sticky header carries an outline search field — the same shear risk.
  // Guard it with the clip oracle rather than force it into the wrapper.
  await expectNoClippedText(page, testInfo, '.picker');
});

// The gloss opens in place, under its own word — no overlay, so it can't hang off
// a screen edge and needs no dismissal machinery. What CAN break instead: two
// glosses open at once (which would shove the mosaic around), a gloss pushing the
// layout sideways, or the ⓘ selecting the word it was only supposed to explain.
test('emotion picker ⓘ — the gloss opens in place, one at a time @ phone width', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.locator('.entry[data-score="20"]').click();
  await page.locator('button.add-emotions').click();
  await page.locator('.picker').waitFor();

  // The seeded check-in already carries emotions, so the selected footer is
  // present from the start; what matters is that reading never changes it. Wait
  // for the footer to render before counting — otherwise this races the first
  // paint and banks a zero.
  await page.locator('.picker .selected').waitFor();
  const chosen = page.locator('.picker .selected .emo');
  const chosenBefore = await chosen.count();
  expect(chosenBefore).toBeGreaterThan(0);

  const gloss = page.locator('.picker .gloss');
  const curious = page.getByRole('button', { name: 'What Curious means' });
  const absorbed = page.getByRole('button', { name: 'What Absorbed means' });

  await curious.click();
  await expect(gloss).toHaveCount(1);
  await expect(gloss).toHaveText('Eager to explore, learn, or find out more.');

  // Another word's ⓘ replaces the open one rather than stacking under it.
  await absorbed.click();
  await expect(gloss).toHaveCount(1);
  await expect(gloss).toContainText('the hours and the world fall away');

  // An open gloss must not widen the surface.
  await expectNoHorizontalOverflow(page, testInfo, '.picker');
  await expectNoTextOverlaps(page, testInfo, '.picker .body');

  // The same ⓘ closes it.
  await absorbed.click();
  await expect(gloss).toHaveCount(0);

  // Reading a gloss must never select the feeling.
  await expect(chosen).toHaveCount(chosenBefore);
  await expect(page.getByRole('button', { name: 'Add Absorbed' })).toBeVisible();
});

// The one the user asked for by name: tapping a to-do opens the edit sheet — a
// dense form (two mat-button-toggle-groups, notes, two date rows with presets,
// connections, a search box, delete). Everything the overlap check can't catch
// on a static page lives here: this sheet is where a too-wide toggle-group
// spills off the right of a phone. Check both the sheet's contents overlap-free
// AND that nothing in it overflows the sheet horizontally.
test('to-do detail — tapping a to-do opens a clean edit sheet @ phone width', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.goto('/todo');
  // Tap the to-do title (role=button span) — the same gesture a thumb makes.
  await page.getByText('Call the GP', { exact: false }).click();

  // The bottom sheet is the .detail container; wait for its far-down controls so
  // the whole form (not just the header) has laid out before we measure.
  const sheet = page.locator('.detail');
  await sheet.waitFor();
  await page.getByRole('button', { name: 'Delete to-do' }).waitFor();
  await page.getByText('Add connection').waitFor();

  // Scope both measurements to the open sheet — it's the component under test,
  // and an opaque modal over the list would otherwise register false overlaps
  // against the (occluded) list text behind it.
  await expectNoTextOverlaps(page, testInfo, '.detail');
  // The sheet's content must fit the sheet's width, whatever it works out to.
  await expectNoHorizontalOverflow(page, testInfo, '.detail');
  // And the page as a whole must never scroll sideways.
  await expectNoHorizontalOverflow(page, testInfo);

  // Sanity: the fields the form promises are actually rendered in the sheet.
  await expect(sheet.getByLabel('Title')).toHaveValue('Call the GP about the referral letter');
  await expect(sheet.getByText('Timing')).toBeVisible();
});
