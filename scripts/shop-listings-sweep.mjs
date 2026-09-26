#!/usr/bin/env node
// Find Asda listings for catalogue products that have none.
//
// ⚠ Discovery is by name (Asda's search ignores EANs), but a hit is linked ONLY
// when its barcode equals the product's; the rest are reported and skipped. Many
// stay unlinked (own-brand barcodes differ from the OFF EAN; Asda lacks some
// items), which beats a wrong link that looks right.
//
//   ./scripts/shop-listings-sweep.mjs            # dry run
//   ./scripts/shop-listings-sweep.mjs --commit   # record the links
//   ./scripts/shop-listings-sweep.mjs --commit --limit 10 --delay 30
//
// Paced and capped per run: it is someone else's storefront.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

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
// Products already asked about, so a second run does not re-ask: without it a
// miss leaves no trace and every run restarts from the same place.
//
// Outside the repo: it is a record of what is in one person's cupboard, and this
// repository is public.
const statePath = flag('--state', `${process.env.HOME}/.cache/life/shop-sweep.json`);
// A miss is not permanent — a shop starts stocking things, and a barcode gets
// corrected. Re-ask eventually rather than never.
const retryAfterDays = Number(flag('--retry-after', '30'));

const loadState = () => {
  try {
    return JSON.parse(readFileSync(statePath, 'utf8'));
  } catch {
    return {};
  }
};
const saveState = (state) => {
  mkdirSync(dirname(statePath), { recursive: true });
  writeFileSync(statePath, JSON.stringify(state, null, 2));
};

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

const state = loadState();
const askedRecently = (id) => {
  const at = state[`${source}:${id}`];
  return at != null && Date.now() - at < retryAfterDays * 86_400_000;
};

// De-duplicated: two cupboard rows of the same thing are one product to price.
// "Already listed" means a listing ATTACHED at this shop, read from the product
// itself: coverage also counts sightings, which are exactly the products most
// worth linking.
const seen = new Set();
const todo = [];
for (const i of items) {
  if (todo.length >= limit || seen.has(i.product_id) || askedRecently(i.product_id)) continue;
  seen.add(i.product_id);
  const detail = api('GET', `/api/products/id/${i.product_id}`);
  if (detail.listings.some((l) => l.source === source)) continue;
  todo.push({ id: i.product_id, barcode: i.barcode, name: i.name });
}
console.log(
  `${items.length} cupboard rows, ${seen.size} products checked; ` +
    `trying ${todo.length} (limit ${limit}, ${delayMs / 1000}s apart)` +
    (commit ? '' : ' — DRY RUN, nothing will be written'),
);

let linked = 0;
let missed = 0;
for (const [i, p] of todo.entries()) {
  if (i > 0) await sleep(delayMs);
  // The app's own "Find at Asda": product name, a shorter second search when
  // that finds nothing, and a barcode match. It caches what the shop returned.
  let found;
  try {
    found = api('GET', `/api/products/id/${p.id}/find/${source}`);
  } catch {
    // A search that fails is not a product that does not exist. Say so, and
    // leave it for another run rather than recording an absence.
    console.log(`  ?  ${p.id}  search failed  | ${p.name}`);
    continue; // NOT recorded: the shop did not answer, so nothing was learned
  }
  if (commit) {
    state[`${source}:${p.id}`] = Date.now();
    saveState(state);
  }
  if (!found.hit) {
    missed++;
    console.log(`  -  ${p.id}  no listing carries this barcode | ${p.name}`);
    continue;
  }
  const hit = found.hit;
  if (commit) {
    api('POST', `/api/products/id/${p.id}/listings`, {
      source,
      external_id: hit.external_id,
    });
  }
  linked++;
  console.log(`  ${commit ? '+' : '~'}  ${p.id}  ${hit.external_id}  ${hit.price_label ?? '(no price)'} | ${p.name}`);
}

console.log(`\nlinked ${linked}, no barcode match ${missed}`);
if (commit) console.log(`asked-about set: ${Object.keys(state).length} (${statePath})`);
if (!commit && linked > 0) console.log('re-run with --commit to record them');
