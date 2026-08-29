#!/usr/bin/env node
// Run a ShopProvider op against a real logged-in Chrome on this machine, instead
// of the Android app's hidden WebView.
//
// The providers in frontend/src/app/shops are already free of Android: each op
// returns { url, js } and nothing more. The wrapper's only jobs are to load that
// url with the user's cookies, install a document-start shim that captures the
// Bearer the page mints, and hand the extractor an `AndroidShop.result` to
// report through. A debug Chrome does all three, so the same provider code runs
// here — which is what makes shop work developable without the phone.
//
// What this does NOT cover: the Kotlin bridge, the full-size-WebView workaround
// for Cloudflare, and the phone's WireGuard DNS. Those still need the device.
// They also change rarely; the extractor JS is the part that churns, which is
// exactly the part this exercises.
//
//   node --experimental-strip-types scripts/shop-desktop.mjs waitrose search "black peppercorns"
//   node --experimental-strip-types scripts/shop-desktop.mjs waitrose product 785492
//
// Requires the ChromeDebug profile running (xinutec-infra/mac-mini/chrome-debug.sh
// start) and, for anything past a search, a hand-made login to the shop in it:
// signed out, Waitrose mints no Bearer at all and every product op returns
// "no token".
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const CDP = process.env.CDP_PY
  ?? `${process.env.HOME}/Code/xinutec-infra/mac-mini/browser/cdp.py`;

// The same patch MainActivity.kt injects at document start, plus the bridge the
// extractor reports through. Kept byte-comparable to the Kotlin so a shop that
// starts failing here is failing there too.
const INJECT = `
(function () {
  if (window.__shopCapInit) return; window.__shopCapInit = 1; window.__authToken = null;
  function isAuth(k) { return String(k).toLowerCase() === 'authorization'; }
  function ra(h) { if (!h) return null;
    if (typeof h.get === 'function') return h.get('authorization') || h.get('Authorization');
    if (Array.isArray(h)) { for (var i = 0; i < h.length; i++) { if (isAuth(h[i][0])) return h[i][1]; } return null; }
    for (var k in h) { if (isAuth(k)) return h[k]; } return null; }
  var of = window.fetch;
  window.fetch = function (u, o) { try { var a = ra(o && o.headers); if (a) window.__authToken = a; } catch (e) {} return of.apply(this, arguments); };
  var os = XMLHttpRequest.prototype.setRequestHeader;
  XMLHttpRequest.prototype.setRequestHeader = function (k, v) { try { if (isAuth(k)) window.__authToken = v; } catch (e) {} return os.apply(this, arguments); };
})();
window.AndroidShop = { result: function (s) { window.__shopOut = s; } };
window.__shopOut = null;
`;

// Waitrose and Asda both serve a different bundle to mobile Chrome, and the APK
// presents one to get past Cloudflare — so this presents the same thing.
const MOBILE_UA =
  'Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 '
  + '(KHTML, like Gecko) Chrome/151.0.0.0 Mobile Safari/537.36';

const [shop, op, arg] = process.argv.slice(2);
if (!shop || !op) {
  console.error('usage: shop-desktop.mjs <waitrose|asda> <search|product|facts> [arg]');
  process.exit(2);
}

const { WAITROSE } = await import('../frontend/src/app/shops/waitrose.ts');
const { ASDA } = await import('../frontend/src/app/shops/asda.ts');
const provider = { waitrose: WAITROSE, asda: ASDA }[shop];
if (!provider) throw new Error(`unknown shop: ${shop}`);
const fn = provider[op];
if (typeof fn !== 'function') throw new Error(`${shop} has no ${op} op`);

const { url, js } = fn.call(provider, arg ?? '');
const dir = mkdtempSync(join(tmpdir(), 'shop-desktop-'));
const jsFile = join(dir, 'extractor.js');
const injectFile = join(dir, 'inject.js');
writeFileSync(jsFile, js);
writeFileSync(injectFile, INJECT);

const out = execFileSync(CDP, [
  'run', url,
  '--js', jsFile,
  '--inject', injectFile,
  '--ua', MOBILE_UA,
  '--mobile',
  '--result-js', 'window.__shopOut',
  // Its own tab, not whichever one is in front: the default is the
  // most-recently-active tab, so this used to navigate away from whatever a
  // person was reading. Same Chrome profile either way, so a hand-made shop
  // login still applies — that is what makes the product op work at all.
  '--new-tab',
], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });

// The extractor reports a JSON *string*, and `cdp.py run` emits a string result
// verbatim — so this parses once. Printed re-indented so a failure reads as one
// line rather than a wall of escapes.
console.log(JSON.stringify(JSON.parse(out), null, 2));
