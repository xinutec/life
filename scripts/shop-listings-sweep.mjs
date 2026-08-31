#!/usr/bin/env node
// Find the shop listing for catalogue products that have none, so "where can I
// buy this" and "cheapest shop" have something to compare.
//
// Measured 2026-08-31: 78 products, 77 with a barcode, and SIX with any shop
// listing. The prices those views need were never gathered — not withheld,
// never collected.
//
// ⚠ NAME to discover, BARCODE to confirm. Asda's search does not match EANs
// (a barcode query returns nothing), so discovery has to go by name — and a
// name is fuzzy enough that bulk-linking on it would manufacture wrong links at
// scale. That is exactly how an oyster sauce came to be filed as honey (#1281),
// and doing it 72 times is worse than doing it once. So a hit is accepted ONLY
// when its barcode equals the product's, and everything else is reported and
// skipped rather than guessed at.
//
// Sampled hit rate under that rule: 4 of 6. Two thirds of the catalogue should
// find itself; the rest stays unlinked, which is the correct outcome for a
// product this cannot identify.
//
//   ./scripts/shop-listings-sweep.mjs            # dry run: says what it WOULD link
//   ./scripts/shop-listings-sweep.mjs --commit   # actually records them
//   ./scripts/shop-listings-sweep.mjs --commit --limit 10 --delay 30
//
// Paced on purpose and capped per run: this is somebody else's storefront, and a
// burst of 72 searches is a scraper. The default spreads a full sweep over
// several runs; --limit exists so it can be run little and often.
import { execFileSync } from 'node:child_process';

const API = new URL('./life-api.mjs', import.meta.url).pathname;
const argv = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = argv.indexOf(name);
  return i === -1 ? fallback : argv[i + 1];
};
const commit = argv.includes('--commit');
const limit = Number(flag('--limit', '25'));
const delayMs = Number(flag('--delay', '20')) * 1000;
const source = flag('--source', 'asda');

if (source !== 'asda') {
  // Waitrose has no server-side search: its provider runs in a browser
  // (scripts/shop-desktop.mjs), so a sweep would have to drive ChromeDebug for
  // every product. Refused rather than half-done.
  throw new Error(`only asda can be swept server-side; ${source} needs the browser path`);
}

const api = (method, path, body) =>
  JSON.parse(
    execFileSync(API, body === undefined ? [method, path] : [method, path, JSON.stringify(body)], {
      encoding: 'utf8',
      maxBuffer: 32 * 1024 * 1024,
    }),
  );

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The catalogue has no list-all endpoint — `/api/products?q=` is a substring
// search capped at 20. So the set comes from the CUPBOARD, which is the better
// set anyway: a price is worth gathering for something you own, not for every
// row the catalogue happens to hold.
const items = api('GET', '/api/items').filter((i) => i.product_id && i.barcode);

// One call says which already have a listing, rather than a product-detail
// fetch each. Asking a shop about something already linked would spend somebody
// else's bandwidth to learn nothing.
const covered = new Set(
  api(
    'POST',
    '/api/shopping/coverage',
    items.slice(0, 200).map((i) => ({
      key: String(i.product_id),
      barcode: i.barcode,
      product_id: i.product_id,
    })),
  )
    .filter((r) => r.sources.some((s) => s !== 'off'))
    .map((r) => r.key),
);

// De-duplicated: two cupboard rows of the same thing are one product to price.
const seen = new Set();
const todo = items
  .filter((i) => !covered.has(String(i.product_id)) && !seen.has(i.product_id) && seen.add(i.product_id))
  .map((i) => ({ id: i.product_id, barcode: i.barcode, name: i.name }))
  .slice(0, limit);

console.log(
  `${items.length} cupboard rows, ${covered.size} already listed; ` +
    `trying ${todo.length} (limit ${limit}, ${delayMs / 1000}s apart)` +
    (commit ? '' : ' — DRY RUN, nothing will be written'),
);

let linked = 0;
let ambiguous = 0;
let missed = 0;
for (const [i, p] of todo.entries()) {
  if (i > 0) await sleep(delayMs);
  let hits = [];
  try {
    hits = api('GET', `/api/products/shop/${source}?q=${encodeURIComponent(p.name ?? '')}`);
  } catch {
    // A search that fails is not a product that does not exist. Say so, and
    // leave it for another run rather than recording an absence.
    console.log(`  ?  ${p.id}  search failed  | ${p.name}`);
    continue;
  }
  const exact = hits.filter((h) => h.barcode === p.barcode);
  if (exact.length === 0) {
    missed++;
    console.log(`  -  ${p.id}  ${hits.length} hits, none with this barcode | ${p.name}`);
    continue;
  }
  if (exact.length > 1) {
    // Two listings sharing a barcode is the shop's ambiguity, not ours to
    // resolve by picking the first.
    ambiguous++;
    console.log(`  !  ${p.id}  ${exact.length} listings share this barcode | ${p.name}`);
    continue;
  }
  const hit = exact[0];
  if (commit) {
    api('POST', `/api/products/id/${p.id}/listings`, {
      source,
      external_id: hit.external_id,
    });
  }
  linked++;
  console.log(`  ${commit ? '+' : '~'}  ${p.id}  ${hit.external_id}  ${hit.price_label ?? '(no price)'} | ${p.name}`);
}

console.log(`\nlinked ${linked}, no barcode match ${missed}, ambiguous ${ambiguous}`);
if (!commit && linked > 0) console.log('re-run with --commit to record them');
