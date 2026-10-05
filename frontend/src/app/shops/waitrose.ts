import type { ShopProvider } from '../shop';

// Extractor JS for the hidden WebView on waitrose.com: reads `window.__authToken`,
// reports via `AndroidShop.result(...)`. Here, so a site change is a deploy.

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
    // [^}]*? spans the keys between lineNumber and name within one object.
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
    if (!tok) {
      // Signed out, Waitrose mints no Bearer: report the likely cause.
      AndroidShop.result(JSON.stringify({ ok: false, reason: 'signed_out',
        error: "the page minted no Authorization header — usually this browser is signed out of waitrose.com; log in by hand and retry" }));
      return;
    }
    var r = await fetch("https://www.waitrose.com/api/products-prod/v1/products/${lineNumber}?view=SUMMARY",
      { headers: { accept: "application/json", authorization: tok }, credentials: "include" });
    if (r.status !== 200) { AndroidShop.result(JSON.stringify({ ok: false, error: "status " + r.status })); return; }
    var j = await r.json();
    var p = (j.products && j.products[0]) || null;
    if (!p) { AndroidShop.result(JSON.stringify({ ok: false, error: "not found" })); return; }
    var im = p.images || {};
    var pr = p.pricing || {};
    // The pack is on 'weights': sizeDescription "42g".
    var w = p.weights || {};
    // A single-item offer's own price wins (the regular one shows as current);
    // a multi-buy is no price for one item.
    var offer = (pr.promotions || []).filter(function (o) {
      return o.promotionUnitPrice && (o.groups || []).every(function (g) { return g.threshold === 1; });
    })[0];
    AndroidShop.result(JSON.stringify({ ok: true, product: {
      source: "waitrose", external_id: p.lineNumber, name: p.name || null, brand: p.brand || null,
      barcodes: p.barCodes || [], quantity_label: w.sizeDescription || null,
      image_url: im.large || im.medium || im.extraLarge || im.small || null,
      display_price: (offer && offer.promotionUnitPrice) ||
        (pr.currentSaleUnitRetailPrice && pr.currentSaleUnitRetailPrice.price) || null,
      // Only to check the number's unit against: the pricing block gives none.
      display_price_label: (typeof pr.displayPrice === "string" && pr.displayPrice) || null,
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
    // Spliced into the JS and the URL.
    if (!/^\d{1,10}$/.test(externalId)) throw new Error('invalid Waitrose lineNumber');
    // A search page reliably mints the token.
    return { url: searchUrl(externalId), js: productJs(externalId) };
  },
};
