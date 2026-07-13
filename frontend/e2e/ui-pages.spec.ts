import { test, expect, type Page } from '@playwright/test';
// The fleet-shared harness, published as @xinutec/ui-harness (source repo
// ~/Code/ui-harness). Ships compiled JS, so it loads straight from node_modules.
import {
  expectNoTextOverlaps,
  expectNoHorizontalOverflow,
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
  { ulid: '01WELLA0000000000000000001', id: 1, recordedAt: hoursAgo(5), score: 2, energy: 2,
    emotions: ['Anxious', 'Withdrawn'], note: 'rough morning', rev: 1, _deleted: false },
  { ulid: '01WELLB0000000000000000002', id: 2, recordedAt: hoursAgo(1), score: 4, energy: null,
    emotions: [], note: null, rev: 2, _deleted: false },
  { ulid: '01WELLC0000000000000000003', id: 3, recordedAt: at(1, 20), score: 3, energy: null,
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
  await page.getByText('Mood · last 14 days').waitFor();
  await page.getByText('Energy · last 14 days').waitFor();
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
});

test('wellbeing — the two charts agree on where the days are', async ({ page }) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.getByText('Energy · last 14 days').waitFor();
  // Mood and energy share one x axis (the same window, the same instant), so a
  // midnight must land on the same pixel in both — otherwise the day rules stagger
  // down the page and the charts can't be read against each other. They align only
  // while the axis column is a fixed width: sized to its words instead, the longer
  // "energetic" would push the energy plot right of the mood plot.
  const plots = page.locator('svg.chart');
  const [mood, energy] = [await plots.nth(0).boundingBox(), await plots.nth(1).boundingBox()];
  expect(mood).not.toBeNull();
  expect(energy).not.toBeNull();
  expect(Math.abs(mood!.x - energy!.x)).toBeLessThan(0.5);
  expect(Math.abs(mood!.width - energy!.width)).toBeLessThan(0.5);
  // This also guards the hand-picked column width: a longer axis word (or a bigger
  // type scale) outgrows the fixed basis, and since a flex item won't shrink below
  // its min-content, the column grows on that chart alone — landing right here as
  // a misalignment rather than as a quiet clip on the phone.
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
  await page.locator('.entry.score-2').click();
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
  await page.locator('.entry.score-2').click();
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

// Two words of one group can be recorded as the single feeling between them. The
// offer appears in the sticky footer, which is the tightest space on the surface —
// so this is where a long prompt would collide with the chips or push the picker
// sideways. It also guards the semantics: the offer must NOT appear for two words
// that merely happen to both be chosen.
test('emotion picker — fusing two words into one feeling @ phone width', async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.goto('/wellbeing');
  await page.locator('.entry.score-2').click();
  await page.locator('button.add-emotions').click();
  await page.locator('.picker').waitFor();
  await page.locator('.picker .selected').waitFor();

  const fuse = page.locator('.picker .fuse .link');
  // The seeded entry carries Anxious + Withdrawn — two feelings from different
  // cores. They are not a blend, and nothing must suggest they are.
  await expect(fuse).toHaveCount(0);

  // Two leaves of one group: Sad › Discouraged.
  await page.getByRole('button', { name: 'Add Disheartened' }).click();
  await page.getByRole('button', { name: 'Add Deflated' }).click();
  await expect(fuse).toHaveCount(1);
  await expect(fuse).toContainText('between Disheartened and Deflated');

  await fuse.click();
  // One chip now, carrying both words — and the two separate chips are gone.
  await expect(page.locator('.picker .selected .emo', { hasText: 'Disheartened–Deflated' })).toHaveCount(1);
  await expect(page.locator('.picker .fuse .link')).toContainText('Split Disheartened–Deflated');
  // Both halves stay lit in the mosaic, at half strength.
  await expect(page.locator('.picker .w.on.half')).toHaveCount(2);

  await expectNoTextOverlaps(page, testInfo, '.picker .selected');
  await expectNoHorizontalOverflow(page, testInfo, '.picker');
  await expectNoHorizontalOverflow(page, testInfo);
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
