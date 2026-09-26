import type { FactsProvider } from '../shop';

// Asda facts provider. Search runs server-side (products::asda), but nutrition,
// ingredients, allergens and dietary tags are only on the product page, behind
// a Cloudflare JS challenge only a real browser passes; hence this WebView
// provider. It returns the page's raw Brandbank blob and EAN, and the server
// interprets them (products::brandbank). Living in the web app, a fix to it is
// a deploy rather than an APK rebuild.
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
