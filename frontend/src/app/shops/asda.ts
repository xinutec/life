import { FactsProvider } from '../shop';

// Asda facts provider — the WebView half of the Asda integration.
//
// Asda's SEARCH runs server-side (a public Algolia call; see products::asda), so
// name/brand/price/pack and the identity all arrive without a browser. What the
// search API does NOT carry is the back-of-pack facts — nutrition, ingredients,
// allergens, the full dietary/free-from set. Those live only on the product PAGE,
// behind Cloudflare, so a plain server fetch is 403'd. Only a real browser passes
// Cloudflare's JS challenge — hence this hidden-WebView provider.
//
// Per the split (backend parses, frontend only fetches): the extractor does the
// minimum that must happen in a browser — waits out Cloudflare, reads the page's
// embedded state, and returns the raw Brandbank blob + the page's EAN untouched.
// All interpretation is server-side (products::brandbank). Kept as a string here
// (in the web app) so Asda page changes are a hot deploy, not an APK rebuild.
const FACTS_JS = `
(async () => {
  // Asda's storefront is a Mobify/SFCC PWA: its server-rendered state, including
  // the Brandbank product content, sits in <script id="mobify-data">. Cloudflare
  // may show a JS-challenge interstitial first; the real WebView clears it and the
  // real page loads, so we poll (this script is re-injected on each page load) for
  // the state to appear rather than assuming it's there on first run.
  function extract() {
    var el = document.getElementById('mobify-data');
    if (!el || !el.textContent) return null;
    var data;
    try { data = JSON.parse(el.textContent); } catch (e) { return null; }
    var st = data && data.__PRELOADED_STATE__;
    var pd = st && st.pageProps && st.pageProps.pageData;
    var ip = pd && pd.initialProduct;
    if (ip && ip.c_BRANDBANK_JSON) {
      return { ean: String(ip.c_EAN_GTIN || ''), blob: String(ip.c_BRANDBANK_JSON) };
    }
    return null;
  }
  try {
    // Up to ~18s (under the native 45s bridge timeout), 500ms apart.
    for (var i = 0; i < 36; i++) {
      var facts = extract();
      if (facts) { AndroidShop.result(JSON.stringify({ ok: true, facts: facts })); return; }
      await new Promise(function (r) { setTimeout(r, 500); });
    }
    AndroidShop.result(JSON.stringify({ ok: false, error: 'no product data on Asda page' }));
  } catch (e) {
    AndroidShop.result(JSON.stringify({ ok: false, error: String(e) }));
  }
})();
`;

/** Asda's slugless product page, keyed by CIN (see products::source). */
function productUrl(cin: string): string {
  return 'https://www.asda.com/groceries/product/' + cin;
}

export const ASDA_FACTS: FactsProvider = {
  id: 'asda',
  facts(externalId: string) {
    // Digits-only guard — the CIN is spliced into the URL.
    if (!/^\d{1,15}$/.test(externalId)) throw new Error('invalid Asda CIN');
    return { url: productUrl(externalId), js: FACTS_JS };
  },
};
