import { ShopProvider } from '../shop';

// Extractor JS runs in the hidden WebView on waitrose.com. Contract with the
// native layer: read `window.__authToken` (captured Bearer), report via
// `AndroidShop.result(JSON.stringify(...))`. Kept as strings here (in the web app)
// so Waitrose site changes are a hot deploy, not an APK rebuild.

// Search: dismiss consent, wait for the server-rendered results, extract
// { lineNumber, name } pairs from the embedded state. No token needed.
const SEARCH_JS = `
(async () => {
  function clickAccept() {
    var b = document.querySelector('.acceptAll');
    if (!b) b = Array.prototype.find.call(document.querySelectorAll('button'),
      function (x) { return /allow all|accept all/i.test(x.innerText || ''); });
    if (b) { b.click(); return true; }
    return false;
  }
  try {
    for (var c = 0; c < 20 && !clickAccept(); c++) await new Promise(function (r) { setTimeout(r, 150); });
    var s = '';
    for (var i = 0; i < 30; i++) {
      s = Array.prototype.map.call(document.querySelectorAll('script'), function (x) { return x.textContent || ''; }).join('\\n');
      if (/"lineNumber":"\\d+"/.test(s)) break;
      await new Promise(function (r) { setTimeout(r, 250); });
    }
    // Other keys (productType, size, …) sit between lineNumber and name in the
    // same object; [^}]*? spans them without crossing objects.
    var re = /"lineNumber":"(\\d+)"[^}]*?"name":"((?:[^"\\\\]|\\\\.)*)"/g, m, seen = {}, out = [];
    while ((m = re.exec(s)) && out.length < 8) {
      var ln = m[1]; if (seen[ln]) continue; seen[ln] = 1;
      var name; try { name = JSON.parse('"' + m[2] + '"'); } catch (e) { name = m[2]; }
      out.push({ external_id: ln, name: name,
        image_url: 'https://ecom-su-static-prod.wtrecom.com/images/products/3/LN_' + ln + '_BP_3.jpg' });
    }
    AndroidShop.result(JSON.stringify({ ok: true, candidates: out }));
  } catch (e) { AndroidShop.result(JSON.stringify({ ok: false, error: String(e) })); }
})();
`;

// Product: dismiss consent, wait for the captured Bearer, call the SUMMARY API
// for the lineNumber, normalize. `lineNumber` is digits-only (guarded below).
function productJs(lineNumber: string): string {
  return `
(async () => {
  function clickAccept() {
    var b = document.querySelector('.acceptAll');
    if (!b) b = Array.prototype.find.call(document.querySelectorAll('button'),
      function (x) { return /allow all|accept all/i.test(x.innerText || ''); });
    if (b) { b.click(); return true; }
    return false;
  }
  try {
    for (var c = 0; c < 20 && !clickAccept(); c++) await new Promise(function (r) { setTimeout(r, 150); });
    for (var i = 0; i < 40 && !window.__authToken; i++) await new Promise(function (r) { setTimeout(r, 250); });
    var tok = window.__authToken;
    if (!tok) { AndroidShop.result(JSON.stringify({ ok: false, error: "no token" })); return; }
    var r = await fetch("https://www.waitrose.com/api/products-prod/v1/products/${lineNumber}?view=SUMMARY",
      { headers: { accept: "application/json", authorization: tok }, credentials: "include" });
    if (r.status !== 200) { AndroidShop.result(JSON.stringify({ ok: false, error: "status " + r.status })); return; }
    var j = await r.json();
    var p = (j.products && j.products[0]) || null;
    if (!p) { AndroidShop.result(JSON.stringify({ ok: false, error: "not found" })); return; }
    var im = p.images || {};
    var pr = p.pricing || {};
    AndroidShop.result(JSON.stringify({ ok: true, product: {
      source: "waitrose", external_id: p.lineNumber, name: p.name || null, brand: p.brand || null,
      barcodes: p.barCodes || [], image_url: im.large || im.medium || im.extraLarge || im.small || null,
      display_price: (pr.currentSaleUnitRetailPrice && pr.currentSaleUnitRetailPrice.price) || null,
      categories: (p.categories || []).map(function (c) { return c.name; })
    } }));
  } catch (e) { AndroidShop.result(JSON.stringify({ ok: false, error: String(e) })); }
})();
`;
}

function searchUrl(term: string): string {
  return 'https://www.waitrose.com/ecom/shop/search?searchTerm=' + encodeURIComponent(term);
}

export const WAITROSE: ShopProvider = {
  id: 'waitrose',
  displayName: 'Waitrose',
  loginUrl: 'https://www.waitrose.com/',
  search(query: string) {
    return { url: searchUrl(query.trim().slice(0, 80)), js: SEARCH_JS };
  },
  product(externalId: string) {
    // Digits-only guard — the id is spliced into the extractor JS and the URL.
    if (!/^\d{1,10}$/.test(externalId)) throw new Error('invalid Waitrose lineNumber');
    // A search page for the lineNumber reliably mints the token; results ignored.
    return { url: searchUrl(externalId), js: productJs(externalId) };
  },
};
