import { test, expect, type Page } from '@playwright/test';
// The fleet-shared harness, published as @xinutec/ui-harness (source repo
// ~/Code/ui-harness). Ships compiled JS, so it loads straight from node_modules.
import type { Item, ItemFile } from '../src/app/models';
// The real vocabulary, not a copy of it: the calendar fixture needs BREADTH
// and a hand-written word list cannot keep it (see CAL_VOCAB).
import { EMOTION_NODES } from '../src/app/shared/emotion-wheel';
import {
  expectNoTextOverlaps,
  expectNoHorizontalOverflow,
  expectNoClippedText,
  expectNoOccludedControls,
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

/** The two places on the wellbeing screen whose geometry is deliberately wider
 *  than the phone, and which therefore have to be exempted from the page-overflow
 *  oracle — narrowly, by element, so the check stays strict on everything else in
 *  the chart (axis words sliding off the left edge is exactly what it caught once).
 *
 *  `.pan` is the scroll rail: a horizontal scroller is wider than its viewport by
 *  definition, and it holds no content. `path.line` is the trend line, which runs
 *  through readings either side of the window so the curve enters and leaves the
 *  viewport correctly; it is clipped to the plot, so none of that is visible — but
 *  a bounding box measures geometry, not paint. The dots are NOT exempt: they only
 *  ever come from inside the window, so one escaping would be a real fault. */
const CHART_SCROLLERS = ['.pan', 'path.line'];

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
  // Older than the widest window, so the charts have somewhere to pan back TO.
  // Without these the rail is exactly one screen long and the pan test is vacuous.
  { ulid: '01WELLD0000000000000000004', id: 4, recordedAt: hoursAgo(12 * 24), scoreTenths: 30, energyTenths: 30,
    emotions: [], note: null, rev: 4, _deleted: false },
  { ulid: '01WELLE0000000000000000005', id: 5, recordedAt: hoursAgo(18 * 24), scoreTenths: 50, energyTenths: 40,
    emotions: [], note: null, rev: 5, _deleted: false },
  { ulid: '01WELLF0000000000000000006', id: 6, recordedAt: hoursAgo(24 * 24), scoreTenths: 10, energyTenths: 10,
    emotions: [], note: null, rev: 6, _deleted: false },
  { ulid: '01WELLG0000000000000000007', id: 7, recordedAt: hoursAgo(30 * 24), scoreTenths: 40, energyTenths: 20,
    emotions: [], note: null, rev: 7, _deleted: false },
];

// A dense stretch for the emotion calendar: the grid is seven columns of squares
// that must survive a phone, and seven scattered readings would not exercise it.
// Deterministic (index-driven, no randomness) so the layout under test is the
// same one every run, and shaped like the real log rather than uniformly —
// mostly one family, a mixed run, varying readings per day, and two gaps,
// because a grid that only ever sees full days never renders the empty box.
const CAL_FAMILIES = [
  ['Happy/Calm'],
  ['Happy/Calm', 'Happy/Productive'],
  ['Happy/Joyful', 'Bad/Sleepy'],
  ['Happy/Steady', 'Sad/Fragile', 'Bad/Pressured'],
  ['Sad/Low', 'Neutral/Uncertain'],
  ['Happy/Mending', 'Sad/Fragile', 'Fearful/Worried', 'Bad/Drained', 'Angry/Annoyed'],
  ['Neutral/Flat'],
  ['Happy/Relaxed', 'Happy/Present'],
];
// ⚠ BREADTH, and it must come from the wheel rather than from a list here.
// CAL_FAMILIES alone names 12 distinct words across 8 repeating sets, so the
// vocabulary saturated after eight days: selecting a week and selecting the
// whole 78-day log produced an IDENTICAL selection panel, 12 chips either way.
// The fixture built to stress that panel therefore could not grow it, and the
// panel-overflow bug was invisible to it — measured on the real log the same
// day, 75 days carried 76 distinct words and a panel 125% of the phone's
// height. Drawing from the wheel makes the tail track the real vocabulary,
// including any word added to it later.
const CAL_VOCAB = EMOTION_NODES.map((n) => n.token);
const CALENDAR_WELLBEING = Array.from({ length: 80 }, (_, i) => 80 - i)
  // Two gaps, so a skipped day renders beside a busy one.
  .filter((back) => back !== 17 && back !== 44)
  .flatMap((back, day) => {
    const perDay = (day % 4) + 1;
    const families = CAL_FAMILIES[day % CAL_FAMILIES.length];
    // A long tail on top of the shaped families, strided across the wheel so
    // each day contributes words the others mostly do not. This is HARSHER than
    // his log rather than a model of it, which is the point: a bound that holds
    // here holds at 76.
    const tail = [0, 1, 2].map((n) => CAL_VOCAB[(day * 13 + n * 29) % CAL_VOCAB.length]);
    const words = [...new Set([...families, ...tail])];
    return Array.from({ length: perDay }, (_, k) => ({
      ulid: `01CAL${String(day).padStart(3, '0')}${String(k)}`.padEnd(26, '0'),
      id: 1000 + day * 10 + k,
      // ⚠ Pinned to a fixed hour of its OWN day with `at`, NOT offset from now
      // with `hoursAgo`. It was `hoursAgo(back * 24 - k * 3)`, and that drifts:
      // the k-offset is measured from whatever time the suite runs, so once the
      // clock passes ~15:00 the later readings of a day cross local midnight and
      // land on the day before. They landed in the deliberate GAPS, so
      // `.box.empty` went to zero and the test failed on the same commit that
      // had passed an hour earlier. A fixture keyed to the wall clock is not a
      // fixture.
      recordedAt: at(back, 9 + k * 3),
      // Swings on the mixed days, flat on the plain ones — the range bar has to
      // be absent most of the time or it says nothing when it appears.
      // Day 21 carries NO score at all: `scoreTenths` is typed `number` and
      // stored docs exist without one, which drew a bar at NaN% titled
      // `score NaN–NaN` against the real log while every fixture set it.
      // Keyed on `families`, NOT `words`: the tail widens every day past two
      // words, and a range bar on every single day says nothing at all.
      ...(day === 21 ? {} : { scoreTenths: families.length > 2 ? [30, 45, 35, 40][k % 4] : 40 }),
      energyTenths: 40,
      // One day tagged nothing at all: present, pressable, uncoloured. Day 14
      // OMITS the field entirely rather than sending [] — `emotions` is absent
      // from the RxDB schema's `required` list, so a stored doc can lack it, and
      // a fixture that always sets it cannot catch the reader that assumed
      // otherwise. One did, and shipped: the live calendar threw
      // "emotions is not iterable" and rendered an empty state.
      ...(day === 14 ? {} : { emotions: day === 9 ? [] : words.slice(0, (k % words.length) + 1) }),
      note: null,
      rev: 500 + day * 10 + k,
      _deleted: false,
    }));
  });

// ⚠ TYPED, and it has to stay that way. This started as a bare array literal
// and silently lost `expiry_precision` when the field was added: the sheet then
// rendered a toggle with NEITHER side selected, which reads as a broken control
// rather than as a stale mock. A mock that is missing what the server always
// sends does not fail — it lies, and the lie looks like a bug in the code under
// test. `Item[]` turns the next such addition into a compile error, which is
// what the gate's e2e typecheck row is for.
// A long filename is the overflow case here — phones name a scan
// "IMG_20240315_143022_receipt_dishwasher.pdf" without being asked.
const FILES: ItemFile[] = [
  { id: 1, item_id: 3, purchase_id: 9, name: 'receipt.png', mime: 'image/png',
    size_bytes: 307_200, created_at: '2026-09-01T10:00:00Z' },
  { id: 2, item_id: 3, purchase_id: null,
    name: 'IMG_20240315_143022_receipt_dishwasher_manual.pdf', mime: 'application/pdf',
    size_bytes: 2_097_152, created_at: '2026-08-20T10:00:00Z' },
];

const ITEMS: Item[] = [
  { id: 1, product_id: null, name: 'Milk (semi-skimmed)', brand: 'Waitrose Essential', category: 'food',
    quantity: 1, unit: 'bottle', expiry: iso(-1), expiry_precision: 'day', location_id: 2,
    barcode: null, has_image: false },
  { id: 2, product_id: null, name: 'Chicken thighs', brand: null, category: 'food',
    quantity: 500, unit: 'g', expiry: iso(1), expiry_precision: 'day', location_id: 2,
    barcode: null, has_image: false },
  // A medicine box, printed MM/YYYY. Its list line reads "expired since June
  // 2026" rather than a date — the longest string this meta slot ever holds, and
  // therefore the one worth measuring against a phone-width row.
  { id: 3, product_id: null, name: 'Levetiracetam', brand: null, category: 'medication',
    quantity: 1, unit: 'box', expiry: '2026-06-30', expiry_precision: 'month', location_id: 2,
    barcode: null, has_image: false },
];

// An item's audit, as the server orders it: newest first, and mixing the two
// readings of `quantity` — a delta on the `used` row, a level on the others.
/** What the row cost. Present so the dialog's newest section is measured too —
 *  a second list above a timeline that was already the height of the screen. */
const ITEM_PURCHASES = [
  {
    id: 1, item_id: 3, product_id: null, barcode: null, name: 'Greek yoghurt',
    shop: 'Waitrose', amount_minor: 250, currency: 'GBP', quantity: 2, unit: 'l',
    unit_amount_minor: 125, unit_measure: 'L', bought_at: '2026-08-29T09:00:00Z',
  },
];

const ITEM_HISTORY = [
  { id: 12, event: 'used', quantity: 200, location: 'Fridge', at: Date.now() - 3_600_000 },
  { id: 8, event: 'moved', quantity: null, location: 'Fridge', at: Date.now() - 2 * 86_400_000 },
  { id: 3, event: 'added', quantity: 950, location: 'Spice cupboard', at: Date.now() - 9 * 86_400_000 },
];

// Three bins on one morning, which is what the real feed does — and the row
// that has to fit a phone: three names joined on one line.
const BINS = [
  { kind: 'Food waste collection', date: iso(1) },
  { kind: 'Rubbish collection', date: iso(1) },
  { kind: 'Paper and cardboard (blue sacks) collection', date: iso(1) },
  { kind: 'Recycling collection', date: iso(8) },
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
    name_source: 'asda', image_source: null, has_image: false },
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
  facts_by_source: [],
  reconciliation: { fields: [] },
  documents: [],
  purchases: [
    { id: 1, item_id: null, product_id: 42, barcode: '5000328042732', name: 'Oat So Simple',
      shop: 'Waitrose', amount_minor: 425, currency: 'GBP', quantity: 594,
      unit: 'g', unit_amount_minor: 715, unit_measure: 'KG',
      bought_at: '2026-08-20T09:00:00Z' },
  ]
};

/** A product Open Food Facts knows under a cryptic crowd name, that no shop
 *  lists yet — the state the "Find at Asda" lookup exists for. */
const UNLISTED_DETAIL = {
  product: { id: 43, barcode: '5063089281581', name: 'Asda ES Balsamic Modena', brand: 'Asda',
    quantity_label: null, source: 'off', external_id: '5063089281581', name_source: 'off',
    image_source: null, has_image: false },
  listings: [
    { source: 'off', external_id: '5063089281581',
      url: 'https://world.openfoodfacts.org/product/5063089281581', raw_name: 'Asda ES Balsamic Modena' },
  ],
  prices: [],
  facts: { nutrition: null, ingredients: null, allergens: [], dietary: [] },
  facts_by_source: [],
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
/** Compare "September 2026"-style month headings chronologically, so the
 *  calendar's order can be asserted without hardcoding which months a
 *  relative-dated fixture lands in. */
const byMonthLabel = (a: string, b: string): number =>
  Date.parse(`1 ${a}`) - Date.parse(`1 ${b}`);

/** The incremental sync pull, as a route handler. Module-scope so a test that
 *  needs a different fixture than mockApi's can re-route the same collection —
 *  the emotion calendar wants a full log where the trend chart wants seven
 *  readings. */
const syncRoute =
  (docs: unknown[]) =>
  (r: Parameters<Parameters<Page['route']>[1]>[0]) => {
    if (r.request().method() === 'POST') return r.fulfill({ json: [] });
    const since = Number(new URL(r.request().url()).searchParams.get('since') ?? '0');
    // Incremental protocol: only send the seed once, else the pull loops forever.
    const fresh = docs.filter((d) => (d as { rev: number }).rev > since);
    const top = docs.reduce<number>((m, d) => Math.max(m, (d as { rev: number }).rev), since);
    return r.fulfill({ json: { documents: fresh, checkpoint: { rev: top } } });
  };

async function mockApi(page: Page): Promise<void> {
  await page.route('**/api/**', (r) =>
    r.request().method() === 'GET' ? r.fulfill({ json: [] }) : r.fulfill({ status: 204, body: '' }),
  );
  // ⚠ **The catch-all above answers `[]`, and a sync pull needs the batch
  // OBJECT.** `isRecord([])` is true (it only excludes null), so the array
  // sailed past the guard, `documents` came back undefined, and every
  // collection this fixture does not override failed its pull — three of four,
  // in every test in this file, since the suite began. Nothing showed it: the
  // goldens are of a sheet, not the shell, so the error indicator they would
  // have carried was never in frame. A harness that renders the app in a
  // permanently-erroring sync state is not rendering the app anybody sees.
  //
  // Registered AFTER the catch-all so it wins — Playwright matches the most
  // recently added route first. A test needing real rows re-routes its own
  // collection with `syncRoute`, which lands later still.
  await page.route('**/api/sync/**', (r) => {
    if (r.request().method() === 'POST') return r.fulfill({ json: [] });
    const since = Number(new URL(r.request().url()).searchParams.get('since') ?? '0');
    return r.fulfill({ json: { documents: [], checkpoint: { rev: since } } });
  });
  // ⚠ **The catch-all above answers `[]`, and a sync pull needs the batch
  // OBJECT.** `isRecord([])` is true (it only excludes null), so the array
  // sailed past the guard, `documents` came back undefined, and every
  // collection this fixture does not override failed its pull — three of four,
  // in every test in this file, since the suite began. Nothing showed it: the
  // goldens are of a sheet, not the shell, so the error indicator they would
  // have carried was never in frame. A harness that renders the app in a
  // permanently-erroring sync state is not rendering the app anybody sees.
  //
  // Registered AFTER the catch-all so it wins — Playwright matches the most
  // recently added route first. A test needing real rows re-routes its own
  // collection with `syncRoute`, which lands later still.
  await page.route('**/api/sync/**', (r) => {
    if (r.request().method() === 'POST') return r.fulfill({ json: [] });
    const since = Number(new URL(r.request().url()).searchParams.get('since') ?? '0');
    return r.fulfill({ json: { documents: [], checkpoint: { rev: since } } });
  });
  await page.route('**/api/me', (r) => r.fulfill({ json: ME }));
  // No worker in a test run: nothing suggested, nothing pretending to think.
  await page.route('**/api/wellbeing/suggest-emotions', (r) =>
    r.fulfill({ json: { suggestions: [], stale: false, pending: false, thinkingSecs: null } }),
  );
  await page.route('**/api/items*', (r) => r.fulfill({ json: ITEMS }));
  await page.route('**/api/items/*/history', (r) =>
    // Events AND what was paid — the dialog reads both now, and a purchase from
    // a hand-typed row is only reachable here.
    r.fulfill({ json: { entries: ITEM_HISTORY, purchases: ITEM_PURCHASES } }),
  );
  await page.route('**/api/bins', (r) => r.fulfill({ json: BINS }));
  // Not linked: the state that actually renders a card with a button in it.
  await page.route('**/api/nextcloud/connect/status', (r) =>
    r.fulfill({ json: { status: 'not_linked' } }),
  );
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
  const sync = syncRoute;
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
  // Three collections joined onto one line is the longest text Today renders,
  // and the row most likely to spill off a phone.
  await page
    .getByText('Food waste · Rubbish · Paper and cardboard (blue sacks)')
    .waitFor();
  await page.getByText('tomorrow', { exact: true }).waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoClippedText(page, testInfo);
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
  await expectNoHorizontalOverflow(page, testInfo, null, CHART_SCROLLERS);
});

test('wellbeing — after logging, saying more is one tap and the strip still works', async ({
  page,
}, testInfo) => {
  // The check-in is the app's most-used interaction, and logging a bare score is
  // the rare case: 196 of 207 entries were edited after creation. This measures
  // the state that appears straight after a tap, which nothing covered before.
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.getByRole('button', { name: 'Log feeling: good' }).click();

  const more = page.getByRole('button', { name: /Add energy, emotions or a note/ });
  await more.waitFor();
  await expect(more).toBeInViewport({ ratio: 1 });

  await expectNoClippedText(page, testInfo, 'app-wellbeing-checkin');
  await expectNoTextOverlaps(page, testInfo, 'app-wellbeing-checkin');
  // Same exemption the chart test uses: the trend rail is wider than the
  // viewport BY DESIGN (it scrolls), so a whole-page overflow oracle reads it
  // as a fault. Exempted by element, not switched off.
  await expectNoHorizontalOverflow(page, testInfo, null, CHART_SCROLLERS);
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

// Panning is the one part of the chart that source can't show is right: the SVG
// keeps a fixed viewBox and redraws the window in place, so what has to be proven
// on a real render is that the axis words DON'T move with the content, and that
// the drawing doesn't grow as history scrolls through it.
test('wellbeing — the charts pan back through history, and the axis stays put', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.getByText('Mood · last 7 days').waitFor();

  const chart = page.locator('svg.chart').first();
  const words = chart.locator('text.axis-word');
  const xs = () => words.evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().x)));
  const before = await xs();
  const dotsBefore = await chart.locator('circle.dot').count();
  expect(dotsBefore).toBeGreaterThan(0);

  // The history runs a month back, so the rail is several screens long.
  const pan = page.locator('.pan');
  const factor = await pan.evaluate((el) => el.scrollWidth / el.clientWidth);
  expect(factor).toBeGreaterThan(3);

  // All the way back to the first check-in.
  await pan.evaluate((el) => (el.scrollLeft = 0));
  await expect(page.getByText('Mood · last 7 days')).toHaveCount(0);
  await expect(page.locator('.caption').first()).toContainText('–'); // a named range

  // The axis words have not moved a pixel: they belong to the chart, not to the
  // content scrolling under it. This is what the whole design turns on.
  expect(await xs()).toEqual(before);
  // ...and the drawing is still a window's worth, not a month's.
  expect(await chart.locator('circle.dot').count()).toBeLessThanOrEqual(dotsBefore + 2);
  // Both charts moved together — one scroller drives both, so their day rules
  // must still agree (the same assertion as the pinned case, now off in the past).
  const rules = (n: number) =>
    page.locator('svg.chart').nth(n).locator('line.day')
      .evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().x)));
  expect(await rules(1)).toEqual(await rules(0));

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo, null, CHART_SCROLLERS);

  // Changing zoom while panned must re-seat the scroller. The window's end is a
  // timestamp, so it survives the change and the charts still look right — but
  // scrollLeft is still measured against the old rail. The symptom is that the
  // NEXT touch teleports the window, so the assertion is: a scroll event that
  // moves nothing changes nothing.
  //
  // The sequencing matters, and got this wrong once: read the caption while a pan
  // update is still in flight and the wait below resolves on THAT transition
  // instead of the zoom's, which makes the test pass or fail for the wrong reason.
  // So let each change land before provoking the next.
  const caption = page.locator('.caption').first();
  const atOldest = (await caption.textContent())!;
  await pan.evaluate((el) => (el.scrollLeft = (el.scrollWidth - el.clientWidth) * 0.5));
  await expect(caption).not.toHaveText(atOldest); // the pan has landed
  const panned = (await caption.textContent())!;

  await page.locator('mat-button-toggle', { hasText: '14d' }).click();
  await expect(caption).not.toHaveText(panned); // the zoom has landed
  const settled = (await caption.textContent())!;

  await pan.evaluate((el) => el.dispatchEvent(new Event('scroll')));
  await page.waitForTimeout(100); // give a change, if there is one, time to render
  expect(await caption.textContent()).toBe(settled);

  // And back. Panned away, "Now" is the only way home.
  await page.getByRole('button', { name: 'Now' }).click();
  await page.getByText('Mood · last 14 days').waitFor();
  expect(await xs()).toEqual(before);
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

test('plan-a-trip sheet — shop, time and list lay out cleanly @ phone width', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.goto('/shopping');
  await page.getByRole('button', { name: 'Plan a trip' }).click();

  const sheet = page.locator('app-trip-sheet');
  await sheet.waitFor();
  // The date field is the one at risk: `datetime-local` renders a browser
  // widget whose intrinsic width is not ours to set, and it is the thing most
  // likely to push past the right edge of a 412px screen.
  await sheet.locator('input[type="datetime-local"]').waitFor();
  await sheet.getByText('list along', { exact: false }).waitFor();
  // Measured with the form filled, which is the state it is submitted in — and
  // the state where the button carries its longest label.
  await sheet.getByLabel('Shop').fill('Waitrose');
  await expect(sheet.getByRole('button', { name: 'Add to calendar' })).toBeEnabled();

  await expectNoClippedText(page, testInfo, 'app-trip-sheet');
  await expectNoTextOverlaps(page, testInfo, 'app-trip-sheet');
  await expectNoHorizontalOverflow(page, testInfo, 'app-trip-sheet');
});

test('bought sheet — a price per row lays out cleanly @ phone width', async ({
  page,
}, testInfo) => {
  // The row at risk is the price field: its label is the item's NAME, which is
  // arbitrary length and user-supplied, and it sits beside a currency prefix on
  // a 412px screen. "Greek yoghurt (the big tubs)" is the mock's longest, and
  // the sheet grows a field per ticked row, so the list is the other risk.
  await mockApi(page);
  await page.goto('/shopping');
  // By role and name, which also pins the fix that made this possible: these
  // checkboxes bound `[attr.aria-label]`, which lands on the mat-checkbox HOST
  // and never reaches the inner input — so every tick control on Buy, Today and
  // To-do was reaching a screen reader as an unnamed "checkbox".
  await page.getByRole('checkbox', { name: 'Bought Greek yoghurt (the big tubs)' }).check();
  await page.getByRole('button', { name: /add to inventory/ }).click();

  const sheet = page.locator('app-buy-sheet');
  await sheet.waitFor();
  await sheet.getByLabel('Shop').fill('Waitrose');
  await sheet.getByLabel('Greek yoghurt (the big tubs)').fill('3.30');
  await expect(sheet.getByRole('button', { name: 'Record' })).toBeEnabled();

  // The button that finishes the job must be ON the screen when the sheet opens.
  // The clipping/overlap/overflow oracles are all silent about this — they were
  // green while "Record" sat below the fold, because a sheet taller than the
  // viewport is not a clipped element, it is a scrolled one. Measured instead.
  const button = sheet.getByRole('button', { name: 'Record' });
  await expect(button).toBeInViewport({ ratio: 1 });
  await expectNoClippedText(page, testInfo, 'app-buy-sheet');
  await expectNoTextOverlaps(page, testInfo, 'app-buy-sheet');
  await expectNoHorizontalOverflow(page, testInfo, 'app-buy-sheet');
});

test('bought sheet — the not-a-price message lays out cleanly @ phone width', async ({
  page,
}, testInfo) => {
  // The state that adds a line of text under a full-height sheet, and the one a
  // person only sees when they have already made a mistake.
  await mockApi(page);
  await page.goto('/shopping');
  // By role and name, which also pins the fix that made this possible: these
  // checkboxes bound `[attr.aria-label]`, which lands on the mat-checkbox HOST
  // and never reaches the inner input — so every tick control on Buy, Today and
  // To-do was reaching a screen reader as an unnamed "checkbox".
  await page.getByRole('checkbox', { name: 'Bought Greek yoghurt (the big tubs)' }).check();
  await page.getByRole('button', { name: /add to inventory/ }).click();

  const sheet = page.locator('app-buy-sheet');
  await sheet.waitFor();
  await sheet.getByLabel('Shop').fill('Waitrose');
  await sheet.getByLabel('Greek yoghurt (the big tubs)').fill('free');
  await expect(sheet.getByText('Not a price', { exact: false })).toBeVisible();
  await expect(sheet.getByRole('button', { name: 'Record' })).toBeDisabled();

  await expectNoClippedText(page, testInfo, 'app-buy-sheet');
  await expectNoTextOverlaps(page, testInfo, 'app-buy-sheet');
  await expectNoHorizontalOverflow(page, testInfo, 'app-buy-sheet');
});

test('plan-a-trip sheet — the unlinked-calendar way out lays out cleanly @ phone width', async ({
  page,
}, testInfo) => {
  // The dead end is the state worth measuring: it adds a filled panel and a
  // second button to a sheet that was already the height of the screen.
  await mockApi(page);
  await page.route('**/api/calendar/shop-trip', (r) =>
    r.fulfill({ status: 409, json: { error: 'nextcloud not linked' } }),
  );
  await page.goto('/shopping');
  await page.getByRole('button', { name: 'Plan a trip' }).click();

  const sheet = page.locator('app-trip-sheet');
  await sheet.waitFor();
  await sheet.getByLabel('Shop').fill('Asda');
  await sheet.getByRole('button', { name: 'Add to calendar' }).click();

  await sheet.getByText('isn’t connected', { exact: false }).waitFor();
  await expect(sheet.getByRole('button', { name: 'Connect it in Settings' })).toBeVisible();

  await expectNoClippedText(page, testInfo, 'app-trip-sheet');
  await expectNoTextOverlaps(page, testInfo, 'app-trip-sheet');
  await expectNoHorizontalOverflow(page, testInfo, 'app-trip-sheet');
});

test('settings — about card: lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Check for updates' }).waitFor();
  // The Nextcloud card carries the longest prose on the screen, in a paragraph
  // that has to wrap inside a card inside a phone.
  await page.getByText('separate from signing in', { exact: false }).waitFor();
  await page.getByRole('button', { name: 'Connect' }).waitFor();
  // Scoped to `.content`, the same reason the dialog cases scope to their
  // container: the check is pure geometry and cannot see that the bottom nav is
  // opaque, so on any page long enough to run under it every line below the
  // fold reads as a collision. `.content` already reserves the nav's height as
  // padding, so what is under the bar at rest is reachable by scrolling.
  await expectNoTextOverlaps(page, testInfo, '.content');
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoClippedText(page, testInfo, '.content');
  // And the question scoping just gave up — is anything actually UNREACHABLE
  // behind that bar — asked properly, of the buttons that matter.
  await expectNoOccludedControls(page, testInfo);
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
    // The native side is an origin-scoped message port now; its presence is what
    // makes the shop tier render, so the stub only has to be able to receive.
    (window as unknown as { ShopBridge: unknown }).ShopBridge = {
      postMessage: () => {},
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

// The item history: a dialog OVER the item sheet, which is the composition
// worth measuring — two stacked surfaces, and a list whose longest line is a
// sentence ("950g on hand · Spice cupboard") inside a phone-width box.
test('item history dialog — the timeline lays out cleanly @ phone width', async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto('/inventory');
  await page.getByText('Milk (semi-skimmed)').click();
  await page.getByRole('button', { name: 'History' }).click();

  const dialog = page.locator('.mat-mdc-dialog-container');
  await dialog.waitFor();
  // All three readings on screen before measuring: the delta wording, the
  // move's destination, and the level wording with a long place name after it.
  await dialog.getByText('Used 200 bottle').waitFor();
  await dialog.getByText('to Fridge').waitFor();
  await dialog.getByText('950 bottle on hand · Spice cupboard').waitFor();

  await expectNoClippedText(page, testInfo, '.mat-mdc-dialog-container');
  await expectNoTextOverlaps(page, testInfo, '.mat-mdc-dialog-container');
  await expectNoHorizontalOverflow(page, testInfo, '.mat-mdc-dialog-container');
});

/**
 * The expiry label's URGENCY COLOUR, measured rather than inspected.
 *
 * `.expiry.expired` was in the DOM on both screens and correct on one of them.
 * Where the label is a list row's trailing meta — Today's "Expiring soon" card,
 * the one screen where an expired thing has to shout — Material's
 * `.mdc-list-item.mdc-list-item--with-trailing-meta .mdc-list-item__end` is a
 * class more specific and simply won: "expired 1d ago" rendered the same grey at
 * the same weight as "in 1d". Nothing in the source said so, and the class list
 * agreed with the intent all the way down.
 *
 * So this asserts the COMPUTED colour, on both screens, against the theme's own
 * error token rather than a hex — a re-themed app should move both together.
 */
test('an expired item reads as expired — on Today and on All items', async ({ page }) => {
  await mockApi(page);
  for (const path of ['/today', '/items']) {
    await page.goto(path);
    await page.locator('.expiry.expired').first().waitFor();
    const seen = await page.locator('.expiry').evaluateAll((els) => {
      // Resolve the token the same way the browser resolves the rule, so the
      // comparison is colour-to-colour and not hex-to-rgb().
      const probe = document.createElement('span');
      probe.style.color = 'var(--mat-sys-error)';
      document.body.appendChild(probe);
      const error = getComputedStyle(probe).color;
      probe.remove();
      return els.map((e) => {
        const s = getComputedStyle(e);
        return { text: e.textContent?.trim() ?? '', color: s.color, weight: s.fontWeight, error };
      });
    });
    const expired = seen.filter((r) => r.text.startsWith('expired'));
    expect(expired.length, `${path} has an expired row to measure`).toBeGreaterThan(0);
    for (const r of expired) {
      expect(r.color, `${path}: "${r.text}"`).toBe(r.error);
      expect(r.weight, `${path}: "${r.text}"`).toBe('700');
    }
    for (const r of seen.filter((x) => !x.text.startsWith('expired'))) {
      expect(r.color, `${path}: "${r.text}" must not read as expired`).not.toBe(r.error);
    }
  }
});

// Receipts and manuals: a dialog OVER the item sheet with a list whose rows
// carry a filename, a size line and a trailing delete — and an empty state,
// which is what it will actually show until somebody attaches something.
test('files dialog — the empty state and the attach button fit @ phone width', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.route('**/api/items/*/files', (r) => r.fulfill({ json: FILES }));
  await page.goto('/inventory');
  await page.getByText('Levetiracetam').click();
  await page.getByRole('button', { name: 'Files' }).click();

  const dialog = page.locator('app-files-dialog');
  await dialog.waitFor();
  await dialog.getByText('receipt.png').waitFor();
  await dialog.getByRole('button', { name: 'Attach a file' }).waitFor();

  await expectNoClippedText(page, testInfo, 'app-files-dialog');
  await expectNoTextOverlaps(page, testInfo, 'app-files-dialog');
  await expectNoHorizontalOverflow(page, testInfo, 'app-files-dialog');
});

// Recording a purchase for something already owned — the path that did not
// exist until 2026-09-03, so nothing you did not buy through the Buy list could
// carry a price or a date. A dialog OVER the item sheet (two stacked surfaces),
// with a price/date field pair and a suffixed unit, which is the composition
// this file's header names as the classic overflow culprit.
test('purchase dialog — the price/date pair and the months suffix fit @ phone width', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.goto('/inventory');
  await page.getByText('Levetiracetam').click();
  await page.getByRole('button', { name: 'Record a purchase' }).click();

  const dialog = page.locator('app-purchase-dialog');
  await dialog.waitFor();
  await dialog.getByLabel('Shop').fill('Currys');
  await dialog.getByLabel('Price').fill('349.99');
  await dialog.locator('input[type="date"]').waitFor();

  await expectNoClippedText(page, testInfo, 'app-purchase-dialog');
  await expectNoTextOverlaps(page, testInfo, 'app-purchase-dialog');
  await expectNoHorizontalOverflow(page, testInfo, 'app-purchase-dialog');

  // The wrong-unit hint is longer than the one it replaces, and it appears in a
  // subscript slot — the exact place a long `mat-hint` has overflowed before.
  await dialog.getByLabel('Warranty').fill('2.5');
  await dialog.getByText('Whole months').waitFor();
  await expectNoClippedText(page, testInfo, 'app-purchase-dialog');
  await expectNoHorizontalOverflow(page, testInfo, 'app-purchase-dialog');
});

// The item sheet's expiry row is a field and a toggle-group side by side, which
// is the composition the header of this file names as the classic overflow
// culprit. Measured in BOTH states, because they are different widths: the date
// input carries a native picker icon the month one does not.
test('item sheet — the expiry row and its precision toggle fit @ phone width', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.goto('/inventory');
  await page.getByText('Milk (semi-skimmed)').click();

  const sheet = page.locator('app-item-sheet');
  await sheet.waitFor();
  await sheet.locator('input[type="date"]').waitFor();
  await expectNoClippedText(page, testInfo, 'app-item-sheet');
  await expectNoTextOverlaps(page, testInfo, 'app-item-sheet');
  await expectNoHorizontalOverflow(page, testInfo, 'app-item-sheet');

  // A medicine box is printed MM/YYYY, so the month input is a first-class
  // state of this form rather than a corner of it.
  await sheet.getByRole('radio', { name: 'Month' }).click();
  await sheet.locator('input[type="month"]').waitFor();
  await expectNoClippedText(page, testInfo, 'app-item-sheet');
  await expectNoTextOverlaps(page, testInfo, 'app-item-sheet');
  await expectNoHorizontalOverflow(page, testInfo, 'app-item-sheet');
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
  // The two newest things on this page, both unmeasured until now: what was
  // paid (a second money list, above the shop prices) and the picture control.
  await page.getByText('What you paid').waitFor();
  await page.getByRole('button', { name: /picture/ }).waitFor();
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
  await expect(page.locator('.family')).toHaveCount(8);
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

// The emotion calendar is a seven-column grid of squares — the layout class that
// fails by overflowing a phone's right edge, and the one where a long month name
// or a chip row in the selection panel shoves the grid sideways. Rendered with a
// dense fixture (CALENDAR_WELLBEING) because seven scattered readings would leave
// most of the grid empty and prove nothing about how it packs.
test('emotion calendar — the day grid and a selection fit @ phone width', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  // Registered after mockApi so it wins: the calendar needs a full log, not the
  // seven readings the trend chart is built around.
  await page.route('**/api/sync/wellbeing*', syncRoute(CALENDAR_WELLBEING));
  await page.goto('/emotions');

  const grid = page.locator('.cal .grid').first();
  await grid.waitFor();
  // Every month between the first and last reading is drawn, so ~80 days of
  // fixture must produce at least three month grids.
  expect(await page.locator('.cal .month').count()).toBeGreaterThanOrEqual(3);
  // A skipped day renders as a box too. EXACTLY three of them, and naming which
  // three is the point: `> 0` let a drifting fixture put readings into one gap
  // and still pass, and then quietly took both. A count that names the number is
  // the instrument; a count that names a floor is a hope.
  //
  // The two deliberate gaps in CALENDAR_WELLBEING (`back` 17 and 44), plus
  // TODAY — the fixture's newest reading is `back: 1`, so the current day has
  // none, and since #1568 an unwritten today gets an empty square rather than
  // falling off the end of the grid. This number moved from 2 to 3 on purpose;
  // if it moves again, something changed the range and not the fixture.
  expect(await page.locator('.box.empty').count()).toBe(3);
  // ⚠ Boxes that share a class must place their number identically. `.box` is on
  // a <button> for a real day and a <span> for a skipped one, and a button is
  // centred by the user agent where a span is not — so leaning on that default
  // put the number dead centre on 75 boxes and 53px up-and-left on the other 3.
  // Every absolute oracle (overlap, clipping, overflow) passed it: nothing was
  // wrong with any single box, they just disagreed with each other.
  const offsets = await page.locator('.box').evaluateAll((els) =>
    els
      .map((b) => {
        const n = b.querySelector('.num');
        if (!n) return null;
        const B = b.getBoundingClientRect();
        const N = n.getBoundingClientRect();
        return {
          tag: b.tagName,
          dx: (N.left + N.right) / 2 - (B.left + B.right) / 2,
          dy: (N.top + N.bottom) / 2 - (B.top + B.bottom) / 2,
        };
      })
      .filter((v) => v !== null),
  );
  expect(offsets.length).toBeGreaterThan(10);
  // Horizontal tolerance covers one glyph: "9" and "30" are different widths and
  // centring each is correct. Vertical has no such excuse.
  const dxs = offsets.map((o) => o.dx);
  const dys = offsets.map((o) => o.dy);
  expect({
    dxSpread: Math.max(...dxs) - Math.min(...dxs),
    dySpread: Math.max(...dys) - Math.min(...dys),
  }).toEqual({ dxSpread: expect.closeTo(0, 0), dySpread: expect.closeTo(0, 0) });

  // No day may advertise a range it could not compute: NaN reads as a broken app.
  const titles = await page
    .locator('.box')
    .evaluateAll((els) => els.map((e) => e.getAttribute('title') ?? ''));
  expect(titles.filter((t) => t.includes('NaN'))).toEqual([]);

  // Chronological months, and the view jumps to the end on load — the ordering
  // is honest and you still land on today. Asserted together because either one
  // alone is the wrong product: oldest-first without the jump opens on the month
  // you care about least, which is why newest-first was tried first.
  const monthOrder = await page.locator('.cal .month h3').allTextContents();
  expect(monthOrder).toEqual([...monthOrder].sort(byMonthLabel));
  const scroll = await page.evaluate(() => {
    const el = document.scrollingElement ?? document.documentElement;
    return { top: el.scrollTop, max: el.scrollHeight - el.clientHeight };
  });
  // At the end, not merely scrolled: a partial jump would still show September.
  expect(scroll.max).toBeGreaterThan(100);
  expect(scroll.max - scroll.top).toBeLessThan(4);

  await expectViewportIsPhone(page);
  await expectNoHorizontalOverflow(page, testInfo, '.cal');
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoTextOverlaps(page, testInfo, '.cal');

  // Selecting days is the handoff to a render, so the panel it opens is part of
  // the layout: a wide selection wraps its chips rather than pushing the grid.
  const days = page.locator('.box:not(.empty)');
  await days.nth(5).click();
  await days.nth(6).click();
  const sel = page.locator('.selection');
  await sel.waitFor();
  await expect(sel.locator('.emo')).not.toHaveCount(0);
  await expectNoHorizontalOverflow(page, testInfo, '.selection');
  await expectNoTextOverlaps(page, testInfo, '.selection');
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoClippedText(page, testInfo, '.selection');
  // The panel floats over the grid, so the controls under it must stay reachable
  // — the failure mode a sticky footer introduces and the clip oracle cannot see.
  await expectNoOccludedControls(page, testInfo, '.selection');

  // ⚠ TWO assertions, and the second is the one that matters. The panel must
  // stay bounded as the selection grows, AND the fixture must be able to grow
  // it — against the 12-word fixture this replaced, every selection produced
  // the same 12 chips, so a bound check would have passed while the real app
  // put a 125%-of-viewport panel over the calendar. Measured there: 3 days
  // filled 54% of the screen, a week 81%, a month 108%.
  await page.getByRole('button', { name: 'Deselect all' }).click();
  // ⚠ Selected through the DOM, not with real clicks, and that is deliberate.
  // An unbounded panel covers the grid, so a Playwright click fails its own
  // actionability check FIRST and the run reports `locator.click: Test timeout
  // of 90000ms exceeded` — measured, twice, with the cap ablated. That message
  // sends you to the test instead of to the layout. Reachability is still
  // asserted, by the occlusion oracle below, which names the right thing.
  const selectFirst = (n: number) =>
    page.evaluate((count) => {
      document.querySelectorAll<HTMLElement>('.box:not(.empty)').forEach((b, i) => {
        if (i < count && b.getAttribute('aria-pressed') !== 'true') b.click();
      });
    }, n);
  const vh = page.viewportSize()!.height;
  const growth: { days: number; chips: number; pct: number }[] = [];
  for (const k of [1, 3, 7, 30]) {
    await selectFirst(k);
    // The panel's own count is the settle signal — no sleep, and it fails
    // loudly if selection ever stops being one-click-one-day.
    await expect(sel.locator('strong')).toHaveText(`${k} day${k === 1 ? '' : 's'} selected`);
    const m = await sel.evaluate((e) => ({
      h: e.getBoundingClientRect().height,
      chips: e.querySelectorAll('.emo').length,
    }));
    const pct = Math.round((m.h / vh) * 100);
    growth.push({ days: k, chips: m.chips, pct });
    // Checked inside the loop so the failure names the selection size that
    // breached rather than the last one measured.
    expect({ days: k, tookMostOfTheScreen: pct > 40 }).toEqual({
      days: k,
      tookMostOfTheScreen: false,
    });
  }
  // The instrument works: a wider selection really does name more words.
  expect(growth.map((g) => g.chips)).toEqual([...growth].map((g) => g.chips).sort((a, b) => a - b));
  expect(growth.at(-1)!.chips).toBeGreaterThan(growth[0].chips * 3);
  // The calendar keeps the screen: the panel summarising it never takes half.
  expect(growth.filter((g) => g.pct > 40)).toEqual([]);
  // Bounded means scrollable, not truncated — every word stays reachable.
  expect(
    await page.locator('.selection .words').evaluate((e) => e.scrollHeight > e.clientHeight + 1),
  ).toBe(true);
  await expectNoClippedText(page, testInfo, '.selection');
  await expectNoOccludedControls(page, testInfo, '.selection');

  // ⚠ THE CALENDAR IS READ-ONLY, and this is the assertion that keeps it so.
  // Its one control was read as deleting the feelings it lists; it deselects
  // days. Watching for a write is worth more than the label, because a future
  // edit could make the label true.
  const writes: string[] = [];
  page.on('request', (r) => {
    if (r.method() !== 'GET' && r.url().includes('/api/')) writes.push(`${r.method()} ${r.url()}`);
  });
  await expect(sel.locator('.emo')).not.toHaveCount(0);
  await page.getByRole('button', { name: 'Deselect all' }).click();
  await expect(page.locator('.selection')).toHaveCount(0);
  // Days come back selectable, so nothing about the underlying log changed.
  await days.nth(5).click();
  await expect(page.locator('.selection .emo')).not.toHaveCount(0);
  expect(writes).toEqual([]);
});
