import type { FactsProvider } from '../shop';

// Asda's facts are only on its product page, behind a Cloudflare challenge only a
// real browser passes. This returns the page's Brandbank blob and EAN for the
// server to parse.
const FACTS_JS = `
(async () => {
  // The state sits in <script id="mobify-data">; poll, as the challenge page may
  // come first (the script is re-injected on each load).
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
    // Under the native bridge's 45 s timeout.
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

function productUrl(cin: string): string {
  return 'https://www.asda.com/groceries/product/' + cin;
}

export const ASDA_FACTS: FactsProvider = {
  id: 'asda',
  facts(externalId: string) {
    // The CIN is spliced into the URL.
    if (!/^\d{1,15}$/.test(externalId)) throw new Error('invalid Asda CIN');
    return { url: productUrl(externalId), js: FACTS_JS };
  },
};
