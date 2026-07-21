# Life — roadmap & TODO

Living checklist for the Life app. Keep it current: tick items as they ship,
add new ones under the right section. Architecture/rationale lives in
`docs/design/overview.md`; this is the "what's done / what's next" tracker.

## Solved 2026-07-16: offline boot dumped a signed-in user onto the sign-in screen

Recorded because the cause is a fleet-wide pattern (any app with ngsw + raw
`fetch`), and because it shows how a "helpful" heuristic becomes a sign-out.

**Symptom.** Open the app with no network: the shell renders signed-in for a
moment, then drops to "Sign in with Nextcloud" — and stays there on every later
offline launch.

**Cause.** The Angular service worker intercepts every page fetch and answers
network failure with a bodiless synthetic **504** (it never lets the fetch
reject). The sync auth guard (`sync/replication.ts` `guardAuth`) classified any
non-JSON response as "logged out" — a heuristic meant for the stale-cookie
302→login-page case. First replication cycle offline → 504 → `AuthState.lost`
→ the shell dropped `me` AND erased the cached identity from localStorage,
poisoning all later offline boots. A downed backend (ingress HTML error page)
would have triggered the same sign-out.

**Fix.** Classification moved to the shared boundary: `classifyFetchResponse`
(`shared/api-error.ts`) returns the same `ApiFailure` union as the HttpClient
side; auth loss is only a positive signal (401/403, followed redirect, or a
*2xx* non-JSON body). Non-ok non-JSON (the SW 504, ingress error pages) is
offline/server → replication retries quietly. Guards: unit regression tests,
an e2e that boots offline signed-in and asserts the shell survives the first
sync cycle (`e2e/offline-boot.spec.ts` — red on the old build, green on the
fix), and dev-lint `DL-ANGULAR-FETCH-ERROR-CLASSIFIED` fleet-wide (no
auth-shaped decisions on raw Responses outside the classifier).

## Solved 2026-07-13: "State token does not match" on sign-in (stale NC cookies)

Recorded because the *cause* is counter-intuitive and will recur elsewhere in the
fleet — every app here signs in through the same Nextcloud OAuth flow.

**Symptom.** "Sign in with Nextcloud" dead-ends on Nextcloud's *"Access denied —
State token does not match"* (HTTP 403). No callback ever reaches us.

**Cause.** Nextcloud writes the login's `stateToken` into the session named by
whatever session cookie the browser arrives with (`showAuthPickerPage` →
`grantPage`, `core/Controller/ClientFlowLoginController.php`). A long-lived client
keeps NC's cookies for months while NC sweeps its sessions — so by the next sign-in
that cookie names a session the server has forgotten, the token dies with it, and
the grant step refuses. A **cookie-less** browser skips the whole path (`base.php`
guards on `count($_COOKIE) > 0`), which is why a fresh install works and a
long-lived one does not.

Reproduced in **desktop Chrome** by planting a dead NC session cookie in a fresh
incognito context — so it is *not* a WebView quirk, and every earlier theory
(no NC session / cross-site SameSite hop / WebView cookie handling) was wrong.

Sliding Life sessions make this **more** likely, not less: logins are now rare, so
the NC cookies will almost always be dead by the time you sign in.

**Fix (android/MainActivity).** On a main-frame 403 from the NC host: stop the load,
drop NC's cookies, explain in a banner, and restart the login from the cookie-less
state that works. Once per launch, so a genuinely broken login can't loop. Verified
end-to-end on the Pixel 9 by poisoning the session cookie over CDP.

**Still open:** the same failure hits *plain browsers* (Chrome on the Mac) with no
recovery — the user must clear cookies for the NC host by hand. Fixing that properly
means fixing it upstream in Nextcloud, or having Life's `/login` route bounce
through something that resets the NC session.

## Backup — DONE (nightly restic)

- [x] **Back up the Life DB** — the `life-db` pod is dumped nightly and
  off-sited. `nixos-config/machines/odin/backup-prepare.sh` runs
  `mariadb-dump --single-transaction --all-databases` inside the `life-db` pod
  (via `crictl`), verifies the trailer (`Dump completed`), zstd-compresses, and
  rsyncs it into odin's restic staging; odin's `restic-backups-cluster` timer
  (02:30 UTC) snapshots it and the Mac mini pulls an off-site copy
  (`restic copy`). Not the Mac-mini `hm-agents.nix` path the old note guessed —
  it landed on odin alongside the other cluster DBs. Restore: pull the snapshot,
  `zstd -d`, feed to `mariadb`. (overview §6)

## Shipped

- [x] Nextcloud identity login (OAuth2) + own DB-backed HMAC sessions
- [x] Generic location/item engine (house→room→cupboard→fridge→layer)
- [x] Inventory: register/delete places, add/edit/move/delete items (CRUD)
- [x] Food fields: category, quantity, unit, expiry (stored)
- [x] Recipes: create/delete, ingredients, shopping-list, cook-now
- [x] Shopping list ("Buy" tab): add/tick/remove + buy→inventory loop
- [x] Product lookup: barcode → Open Food Facts, cached in our DB (image as
      BLOB, served from /api); barcode field + thumbnails on Buy/Inventory
- [x] Camera barcode scanner (native BarcodeDetector, graceful fallback) on
      Buy + Inventory → scans the code, fills it, runs the lookup
- [x] Search → location breadcrumb ("where is my X")
- [x] Shopping rows carry `category` + `product_id` (2026-07-16) — the
      buy→inventory conversion uses them instead of guessing; category select
      in the Buy sheet; RxDB v1 migration + server migration 0024
- [x] Inventory→Buy bridge (2026-07-16) — cart button on inventory rows adds
      the item to the Buy list with its full identity, deduped against un-done
      rows (matchesIdentity: product link, then barcode, then name)
- [x] Product picker (2026-07-16) — one "find a product" dialog on both
      sheets' Name fields: inventory tier (instant, offline), catalog tier
      (`GET /api/products?q=`, debounced), shop tier (Waitrose, in-app only;
      picking imports into the catalog). Replaced the Find-on-Waitrose dialog.
- [x] Asda product tier (2026-07-16) — `GET /api/products/shop/asda?q=` searches
      Asda's storefront via their public Algolia index (`src/products/asda.rs`).
      Unlike the Waitrose WebView bridge this is a plain backend call, so the
      Asda tier works in the browser too. Picking imports (source `asda`, keyed
      by CIN; scene7 image fetched server-side); the hit's EAN rides back so the
      row is barcoded. Reverse barcode→product isn't available (IMAGE_ID isn't a
      searchable Algolia attribute) — name search only.
- [x] Product data model, increment 1 — the split (2026-07-16) — canonical
      `products` (keyed by EAN) vs per-source `product_listings` (migration
      0025). Enrichment reconciles sources onto one product by barcode: Asda and
      Open Food Facts describing the same EAN now land on ONE product with two
      listings, instead of two rows (the old flat table's UNIQUE(barcode) made
      that impossible, which is why Asda had been imported barcodeless). Import
      takes an optional `barcode`; Asda passes its IMAGE_ID, Waitrose its
      barCode. `products.source`/`external_id` kept as vestigial origin columns
      (a later migration drops them).
- [x] **Product data model, increment 2 — prices** (2026-07-16) —
      `price_observations` (migration 0026): append-only, INT minor units (never
      float), region-tagged, per-unit for fair comparison. `record_price` +
      `latest_prices` (cheapest-per-shop); `GET /api/products/id/{id}/prices`.
      Asda hits carry a structured price (amount + per-unit + region) and record
      an observation on import. Waitrose price capture deferred until its
      amount unit (pounds vs pence) is confirmed in-app — precision over a 100×
      risk. Subsumes the old "Purchases: shop + price observations" item.
- [x] **Product data model, increment 3 — nutrition + ingredients + allergens +
      dietary flags** (2026-07-16) — `product_nutrition` (migration 0027): the UK
      "big 8" per 100g/ml wide (energy kJ+kcal, fat, saturates, carbs, sugars,
      fibre, protein, salt), with OFF's per-100 tail (sodium, …) kept verbatim in
      a JSON `extra`. `product_allergens` (contains / may_contain) and
      `product_dietary_flags` (vegan/vegetarian/palm-oil tri-state + gluten-free/
      organic/kosher/… label claims); `products.ingredients_text`. All parsed from
      the SAME OFF response the barcode lookup already fetches (`nutrition.rs`
      `RawFacts::parse`, pure + unit-tested) and stored against the canonical
      product, so facts land on the product every source's listings share.
      Stored against the canonical product and served as part of the product
      detail (increment 4). Asda LIFESTYLES enrichment is a later add (facts
      today come from OFF only).
- [x] **Product data model, increment 4 — payoff screen + deep links + clean
      names** (2026-07-16) — the increment that pays the model off: scan a
      barcode, get everything we know on one screen.
      - `GET /api/products/id/{id}` → `ProductDetail` {product, listings, prices,
        facts} in ONE fetch (replaces the `/prices` + `/facts` sub-routes; the
        page needs all of it at once, and three round-trips to render one screen
        was never the plan).
      - **Deep links** derive from listing identity — no stored slug needed
        (probed live 2026-07-16: Asda's PDP is slugless at
        `www.asda.com/groceries/product/{CIN}`, and Waitrose redirects ANY slug
        to the canonical one, keyed by the trailing lineNumber). A stored
        `product_listings.url` still wins when a source supplies one.
      - **Clean names**: `source::NAME_PREFERENCE` (waitrose > asda > off) picks
        the canonical title from the listings' `raw_name`s, deterministically —
        by source quality, never recency, so a later OFF lookup can't clobber a
        retailer's clean title with a crowd one. `refresh_canonical_name` runs
        last on every listing-touching path and tracks provenance in
        `name_source`. No hand-editing (see the no-user-edits rule).
      - **`ShopPrice` is now one row per SOURCE**, its cheapest listing, carrying
        that listing's `external_id` — a shop CAN list one product twice (two
        Asda CINs on an EAN), which made the old per-listing rows contradict the
        type's own "the latest price for one shop" promise.
      - Frontend: `/product/:id` page (hero image, dietary chips, where-to-buy
        cheapest-first with deep links, the UK panel, ingredients, allergen
        chips, OFF attribution). Entry points: "View product" in the
        inventory/buy sheets and "Scan a product" in the hamburger — barcode →
        lookup → page. Failures are classified (a 404 says the product isn't in
        the catalogue; only a real network failure blames the connection).
- [x] **Product data model, increment 5 — "Find at Asda" on the product page**
      (2026-07-16) — attach a shop to a product you already have, without
      detouring through the Buy sheet's picker. Frontend-only: the Asda search
      endpoint and the barcode-reconciling import already existed.
      - **Matched by EAN, never by name or the shop's ranking** (`eanMatch`, pure
        + tested). Neither retailer supports barcode→product, so we can only
        reach them by NAME search — and searching Asda for a product's Open Food
        Facts name ("Asda ES Balsamic Modena") ranks a *raspberry glaze* above
        the product itself. Every hit carries its EAN, so relevance order is
        discarded: the hit whose barcode equals ours IS the product; everything
        else is a different product and cannot be attached. Precision over
        recall, as with visits.
      - Offered only when it can answer honestly: needs a barcode to match on,
        and hidden once Asda already lists the product. No match → an explicit
        "No Asda product carries this barcode" (a Spanish olive oil genuinely
        isn't at Asda; a Filippo Berio is NOT it).
      - Attaching imports under the HIT's own barcode (equal to ours — that's why
        it matched), so the backend reconciles the two; we never force our
        barcode onto a shop's listing. Price + deep link + Asda's clean name
        (via the increment-4 ranking) all follow.
      - Asda only: its storefront search is a public API callable from anywhere.
        Waitrose needs the Android app's WebView to pass its bot-wall, so it
        stays in the picker's shop tier rather than being half-offered here.
- [x] **Product data model, increment 6 — keep what the shop already told us;
      refresh on demand** (2026-07-17) — everything fetched was already stored
      (the product page makes ZERO shop calls), but each Asda hit carried data we
      were binning, and a second fact source would have been erased.
      - **Migration 0028**: `product_dietary_flags` keyed
        `(product_id, source, flag)`, replace scoped per source. 0027 keyed it
        `(product_id, flag)` with a delete-by-product — fine for ONE authority
        (OFF), fatal for two: storing Asda's tags then re-looking-up the barcode
        on OFF would silently DELETE them.
      - **Merge on read** (`nutrition::merge_dietary`, pure + tested): sources
        agree → that value; a firm claim settles a guess ('yes' over 'maybe');
        **'yes' against 'no' → 'maybe'** — over-claiming is the harmful
        direction, and the tri-state exists so we needn't pick a side.
      - **Asda's `NUTRITIONAL_INFO`** → dietary flags (Vegan/Vegetarian/Halal/
        Kosher/NoGluten→gluten_free/NoLactose→lactose_free). **A 0 is NOT a
        "no"** — Asda ships all 24 tags on every product and sets what it claims
        (Quaker oats really do carry `Vegetarian: 0`), so flags only ever assert
        'yes'. Its other tags (LowSalt, HighFibre, NoNuts…) have no slug in our
        vocabulary and are dropped rather than invented. `PACK_SIZE` →
        `quantity_label` when OFF gave none.
      - **`POST /api/products/id/{id}/listings`** — one idempotent server-side
        pull that both attaches a shop and refreshes it, so a refresh can never
        capture less than an attach. Fetches by CIN (`asda::fetch_by_id`; the CIN
        IS searchable, and the hit's own CIN is verified — a search is a
        relevance guess, this must be an identity) and re-checks the barcode
        server-side rather than trusting the client's match.
      - **Refresh is manual, by design**: a button per Asda row, no cron, no
        staleness check, nothing on load. Shop data goes stale silently; a wrong
        price you didn't ask for is worse than an old one you can refresh.
- [x] **Product data model, increment 7a — remember every shop query, look it up
      before asking again** (2026-07-17) — a shop query returns far more than the
      product that prompted it: one Asda search hands back ~15 hits, each with
      its own EAN. We read the one that matched and dropped the rest, then paid
      for a fresh search next time. Those were durable barcode → CIN facts, bought
      and binned.
      - **Migration 0029 — `shop_listings`**: our memory of the shops'
        catalogues, keyed `(source, external_id)`, barcode-indexed. Deliberately
        NOT `product_listings` (whose `product_id` is NOT NULL — storing a hit
        there would mint a catalogue `products` row, and an image blob, for every
        incidental result). Image is a URL, never bytes. A row graduates into a
        real `product_listings` row only when matched to a product and attached.
      - **`shop_cache::remember`** stores every hit a search returns;
        `search_asda` and the new find endpoint both write through it. Upsert on
        the shop's identity; **`COALESCE(VALUES(x), x)`** so a thinner re-sighting
        (a Waitrose *search* hit, which carries no barcode) never erases what a
        fuller one taught us — the same silent-erasure shape as inc 6, guarded the
        same way and proved by a fault-injection test.
      - **`GET /api/products/id/{id}/find/{source}`** — memory first, shop
        second. A cache hit answers with ZERO outbound traffic; only a miss
        searches, and that whole result is remembered on the way back, so lookups
        tend toward no queries as the cache fills. `ShopFind { hit, from_cache }`
        — the UI shows *"already knew this one"* so a cache you can't see can't be
        wrong unnoticed.
      - **Match moved server-side** (`asda::match_barcode`, pure + tested, the
        raspberry-glaze case ported from the frontend's `eanMatch`): identity is
        the barcode, never the shop's relevance order. No cap or sampling — a
        `None` means every hit was checked and none carried this EAN, a real
        answer, not "gave up early".
      - This caches OUR OWN queries only — never a crawl of a shop's catalogue.
      - **7b (next): "Find at Waitrose"** on the product page, via the Android
        bridge (server can't pass the bot-wall). Its search hits carry no
        barcode, so it fetches candidates until one matches — uncapped, cache
        first, app-only. The cache + `find` endpoint are already shop-agnostic;
        7b wires the bridge into `shop_cache::remember` and a Waitrose provider.
- [x] **Product data model, increment 8 — store the whole record; reconcile the
      canonical row by approval** (2026-07-18) — a source's account of a product
      used to be flattened onto the canonical row (silently overwriting name/brand,
      silently ignoring pack/image) with most of its payload dropped at parse time.
      Now each source keeps its whole record on its own line, and the canonical
      row is a curated choice you approve.
      - **Migration 0030** — `product_listings` gains `brand`, `quantity_label`,
        `image_url`, and `raw_json` (the source's ENTIRE record, verbatim), so a
        field we don't model yet is kept rather than lost. `asda.rs` carries the
        untouched hit JSON onto `AsdaHit.raw` (off-wire, off the TS bindings) and
        maps the free-from lifestyle tags too (nut/milk/egg/soya-free, organic);
        nutrition CLAIMS (LowSalt/LowFat/HighFibre) stay in `raw_json`, not
        promoted to dietary flags. Asda's SEARCH payload carries no per-100g
        nutrition / allergens / ingredients (confirmed against the captured
        fixture), so among facts only dietary has two sources today.
      - **Fill-if-empty, never silent-overwrite**: `refresh_canonical_name` only
        seeds an empty name; a differing source becomes a divergence, not an
        automatic switch.
      - **Migration 0031 — `product_field_decisions`**: a divergence is computed
        LIVE (listing value vs canonical), so there's no pending-changes table to
        rot. A decision records the exact value SET it settled, so it re-surfaces
        only when a source's value actually changes — the conflict-log resolve
        idea, keyed by (product, field).
      - **`POST /api/products/id/{id}/reconcile`** — `divergences()` (pure, over
        name/brand/quantity_label) rides on `ProductDetail`; reconcile adopts a
        source's value or keeps the current, per field. The product page shows a
        "Shops disagree" panel with a radio per field (default keep) and one
        Apply — the 250ML-vs-250ml case surfaced instead of guessed.
      - **8b — our own editable name** (2026-07-18): reconcile accepts
        `choice='user'` + a typed value → sets the canonical name with
        `name_source='user'`, which outranks every source, settles the divergence,
        and is never auto-overwritten (guarded on the barcodeless re-import path
        too). Product page has a pencil-edit on the name. Built for "Oalty": Asda's
        catalogue literally has the typo, OFF has a crowd title, so NO source spells
        "Oatly" — the source-only picker couldn't fix it; our own layer can. The
        shops keep their own honest spelling on their listings.
      - **8c — our own editable brand + pack size** (2026-07-18): the same `user`
        mechanism for the other two reconcilable scalars. **Migration 0032** adds
        `brand_source` / `quantity_label_source` (mirroring `name_source`), so a
        typed brand/pack survives a source refresh. Product page has a pencil-edit
        on the subtitle (both fields in one inline form; only changed fields are
        sent). This closes the original observation — "Asda · 250ML" was the shop's
        own casing no source disagreed with, so only our layer could fix it.
      - **8d — Asda page facts (nutrition/ingredients/allergens/dietary)** (2026-07-18):
        Asda's SEARCH API carries no facts; they live on the product page behind
        Cloudflare (probed: a plain server fetch is 403'd — "Just a moment…"),
        readable only through the Android app's hidden WebView. Shipped in three:
        - **F1 (migration 0033)**: nutrition/allergens/ingredients become
          per-source (like 0028's dietary), so Asda's and OFF's coexist. Merge on
          read: nutrition/ingredients pick by source precedence, allergens UNION
          (most-severe presence wins — an allergen one source declares is never
          dropped), dietary the existing tri-state merge.
        - **F2 (`brandbank.rs` + `POST /products/id/{id}/facts`)**: parse Asda's
          `c_BRANDBANK_JSON` blob server-side → ProductFacts; barcode-gated (the
          page's `c_EAN_GTIN` must match the product), then `store_facts(…,'asda')`.
          Brandbank `false` booleans are NOT read as firm "no" (same caution as the
          search tags) — only `true` → a 'yes' flag.
        - **F3 (`shops/asda.ts` FactsProvider + product-page button)**: the hidden
          WebView returns the raw blob (frontend fetches, backend parses); an in-app
          "Get full details from Asda" action (bridge-gated) posts it. Installed
          Pixel 9 APK v0.5 already allowlists asda.com — no APK rebuild.
      - **F2b — keep every fetched payload verbatim** (2026-07-18, migration 0034):
        anything we fetch, we store as-is so we never fetch it twice (and can
        re-derive from it if the parser improves) — combining is a separate step.
        `product_documents(product_id, source, kind, body, fetched_at)`; `/facts`
        stores Asda's raw page blob (kind='page') before parsing. `ProductDetail.
        documents` lets the page show "Refresh Asda's details · stored {ago}" once
        held. **F2c** then closed the last gap: OFF's response is kept verbatim on
        its own `off` listing's raw_json, so every fetch we make is now archived
        (Asda search + OFF → listing raw_json; Asda page → product_documents).
      - **On-device check (DONE 2026-07-18):** the Asda WebView facts fetch is
        exercised live and reliable on the Pixel 9 — real Brandbank facts fetched,
        parsed, and stored (a 17KB page doc + per-source nutrition/allergens/
        ingredients). Two APK fixes were needed to pass Cloudflare (render the
        hidden WebView full-size behind the app, not 1×1; present a real
        mobile-Chrome UA) plus a retry on transient load errors — MainActivity.kt,
        commit 91f9e6b. GOTCHA: it was flaky until a `DNS = 1.1.1.1` line was added
        to the phone's WireGuard `[Interface]` — the DNS-less split-tunnel VPN left
        the WebView resolver with no nameserver (`ERR_NAME_NOT_RESOLVED`). See
        docs/shop-enrichment or the memory note.
      - **8e/F4 (DONE 2026-07-18): facts reconciliation.** Facts no longer merge
        silently. Nutrition + ingredients (whole-value, non-safety) reconcile by
        SOURCE-PICK through the same approve grammar as the scalar fields: a
        divergence surfaces when sources differ, and the pick is recorded in
        `product_fact_sources` (0035) so the merge honours it and it stays settled.
        Allergens + dietary (safety-critical) are NOT pickable — they keep the safe
        merge (union / tri-state) and instead get read-only per-source PROVENANCE
        on the product page ("Sources differ": who declared which allergen, whose
        vegan claim is which). `ProductDetail.facts_by_source` carries it.
      - **8f (DONE 2026-07-18): picture reconciliation.** The canonical image is
        bytes we hold; a listing offers only a URL — so there's nothing to
        value-compare. Reconciled by PROVENANCE instead: `products.image_source`
        (migration 0036, mirrors name_source) records which source our picture came
        from, and a listing from a DIFFERENT source that offers its own image is a
        candidate to adopt (`repo::picture_divergence`, pure). Adopting re-fetches
        the chosen source's picture through the same SSRF-gated, no-redirect fetch
        the import path uses (handled in the reconcile route, since it's I/O, then
        `set_image_provenance`); keeping just settles it (`settle_picture`, reusing
        `product_field_decisions` keyed on "picture"). A hand-uploaded picture is
        `image_source='user'` and never nagged. The product page renders the choice
        as THUMBNAILS (the current one beside each source's), not URL text. Tests:
        `tests/picture_reconcile.rs` (pure + real-DB) + product.spec.ts. This
        completes the increment-8 reconciliation roadmap.
- [x] **Client activity trace** (2026-07-17) — the navigations and taps the
      browser sees but the API doesn't, folded into the SAME log stream as the
      per-request trace so a session reads as one timeline (`nav /product/56` →
      `tap "Find at Asda"` → `GET …/find/asda 200`). Instrumented ONCE, no
      per-screen code: `Telemetry` (`frontend/src/app/telemetry.ts`, wired in the
      `app.ts` shell) captures from two central seams — Router events (nav) and a
      single global capture-phase click listener that reads the nearest control's
      accessible name (`labelFor`) — batches, and POSTs to `POST /api/telemetry`
      (`src/routes/telemetry.rs`), which only logs them (NO storage; these are
      logs, not data). Best-effort: dropped-not-retried, `sendBeacon` on
      backgrounding, auth-gated (an open log-write would be an injection vector).
      Labels are verbatim — `labelFor` strips `mat-icon`/`[aria-hidden]` text so a
      Material icon's ligature doesn't prefix every button. Read a session with
      `kubectl -n life logs deploy/life-app | grep client-event`.
- [x] **Wellbeing check-in reminders** (2026-07-20) — device-local Android
      notifications that nudge you to check in. A **generic native bridge**
      (`window.ReminderBridge`: `available`/`schedule`/`cancel`, mirroring
      `ShopBridge`) fires an `AlarmManager` exact alarm → a manifest
      `ReminderReceiver` posts the notification even with the app closed; a tap
      deep-links the WebView to the target route (`/today`). Permissions
      `POST_NOTIFICATIONS` (Android 13+ runtime, requested when a reminder is first
      set) + `USE_EXACT_ALARM` (auto-granted — the app is sideloaded, so no
      exact-alarm settings dance); the `<receiver>` rides a dev-lint manifest
      waiver (`allow=application/receiver`). No boot receiver: the "simple" model —
      the web app **re-arms on every open** (subscribing to the wellbeing store),
      so an alarm survives the app closing but is re-armed after a reboot.
      - **Driver**: `WellbeingReminder` (`shared/wellbeing-reminder.ts`) holds a
        device-local list of rules (`{ time, quietHours }`, localStorage
        `life.reminder.wellbeing`), each armed as its own alarm keyed by a stable
        id. `nextFireForRule` (pure, tested) fires at the next occurrence of the
        rule's time that's both in the future AND at least `quietHours` past the
        last check-in — so "remind me at 9am if I haven't logged in 3h, and at 6pm
        if not in 6h" is two rules. Because the only way to check in is in the app
        (which re-arms), a same-day check-in pushes the rule to the next day with no
        server round-trip. Configurable in **Settings** (add/remove rows; time +
        "if no check-in for N h"); the editor shows in a browser but notes reminders
        fire in the Android app (the bridge is absent). No APK-manifest permission
        change reaches the other WebView apps — permissions are a per-app dev-lint
        slot.
- [x] 3D house renders the real `scenes/house.json` (perimeter walls + furniture)
- [x] Mobile-first UI (bottom tabs ↔ side rail), management forms, NC avatar
- [x] Deployed: isis k3s, CI/CD (`xinutec/life`), DNS, TLS, live login
- [x] Wordmark "Life"

## Next up

- [x] **Wellbeing · to-do timing · UI quality** — plan in
      `docs/proposals/wellbeing-timing-ux.md` (2026-07-03). SHIPPED as six
      increments, all deployed: **A** shared `<app-list-state>` + `Feedback`
      service across the 7 list screens; **B** `not_before`/`due` on to-dos
      (waiting + urgency states, sort, chips, presets); **C** wellbeing
      tracking (new sync entity, face check-in, 14-day trend + timeline,
      trash/conflict integration); **D** Today landing screen (default route);
      **F** self-hosted fonts + sign-in card + settings mat-list. Universal
      Undo landed inside A (all deletes route through `Feedback.undo`).
      **E and beyond shipped 2026-07-03** after Pippijn delegated UI style
      ("standard over custom, no style check-ins"): **G** `mat-toolbar` shell,
      standard account icon-button, M3 bottom-nav active pill, shell type on
      the M3 scale; **H** one add/edit grammar — FAB → bottom sheet on
      Buy/Inventory/Recipes/To-do (`<app-sheet-header>` + global `.sheet-form`;
      add-sheets stay open for burst entry, edit-sheets close on save; to-do
      TYPES/PRIORITIES deduped into `todo-meta.ts`); **I** M3 type scale
      everywhere + one global `.pill`/`.expiry` grammar; **J** actionable
      Today rows (tick with Undo, tap → detail sheet) + a Playwright
      rendered-pixels gate (`npm run ui-check`, e2e/ui-pages.spec.ts: 390px,
      mocked busy data, no-text-overlap assertion — caught the `nutrition`
      mat-icon rendering as literal text on day one; classic Material Icons
      has no such glyph).
- [ ] **Expiry / "use soon"** — surfaced as the "Expiring soon" card on Today
      (increment D; expired/soon colouring via the global `.expiry` grammar).
      Kept open only for: is a dedicated fuller view wanted beyond the card?
- [~] **Extend `scenes/house.json` to the whole house** — built collaboratively
      against the local preview (Pippijn measures each piece; see
      `scenes/README.md` § "Live modelling workflow"). **Kitchen: both long walls
      DONE 2026-07-03** — cooking run (worktop, hob, recessed sink+drainer,
      dishwasher) + utility wall (larder, fridge, over-fridge cabinet, serving
      hatch with pass-through worktop, base cabinet, divider, open shelves, mug
      cubbies) + hall doorway + hatch cut through both wall layers. **Remaining in
      the kitchen: oven/drawers tower, extractor hood, back door.** Then the other
      rooms (dining, hall, upstairs); decide how rooms compose (shared origin /
      offsets).
- [ ] **Place cupboards in scene coordinates** → **"where is my X" → highlight
      in 3D** (parked: the demo box-highlight AND the item-name search page were
      both removed — 2026-07-02). Rebuild the lookup together with the highlight;
      decide how DB locations map to scene geometry. `ancestor_path` (the 2D
      breadcrumb helper) was removed with search; reinstate it here if needed.
- [ ] **CalDAV** — read the Brent bins feed; write "shop trip" `VEVENT`s with a
      location. Needs the Login-Flow-v2 app-password link (overview §2b, §5).
- [x] **Frontend test runner** — vitest via `ng test` (43 specs as of
      2026-07-02: sw-updates, conflict merge, trash/conflicts screens, todo
      graph, stores, settings, shopping scan).
- [~] **Emotion suggestions in the feelings picker** — a small local model reads
      the check-in note and offers the feelings that fit, at the head of the
      wheel. Personalised: the prompt carries the last 80 of your own labelled
      check-ins as few-shot, which roughly doubled agreement with your real
      picks in an offline eval on held-out entries (13% → 25% on a 3B, 50% on
      Qwen2.5-7B-4bit). Suggestions are cached per check-in (`emotion_suggestions`,
      migration 0037) so an unchanged note answers from a lookup; edit the note
      and the previous set stays on screen, labelled, while the new one runs.
      **Generation is Mac-initiated** (`emotion_jobs` + `/api/emotion-worker/*`,
      `tools/emotion_worker.py`): the model is Apple-Silicon MLX on the Mac, and
      the fleet may not open connections toward it (one-way WireGuard peer), so
      the worker polls for work and posts answers back. The picker only claims to
      be thinking when a worker has actually been seen — no spinner in front of a
      model that isn't there. REMAINING: run the worker on the Mac (launchd agent
      in `deploy/hm-agents.nix`, needs `~/.config/life/worker.env` + the matching
      `EMOTION_WORKER_TOKEN` in `life-secret`).

## Backlog

- [ ] **Product extras** — name+image copied onto items at add-time (currently
      items carry the barcode and the thumbnail is fetched live from the cache —
      fine, but not self-contained if the cache is wiped); camera photo +
      paste-URL→`og:image` as alternative image sources; manual "refresh from
      OFF"; a `@zxing/browser` fallback for non-Chromium browsers (the native
      BarcodeDetector scanner only works on Chromium); contribute missing
      products back to OFF (uses Pippijn's OFF account — creds user-held).
- [ ] **Purchases: shop + price observations** (design decided) — price is NOT
      a product attribute; it varies by shop and time, so model it as an
      **observation = the same record as "where bought"**:
      - A `price_observations` row: `barcode`/product, `shop`, `amount`,
        `currency` (ISO, default GBP), `quantity` + `unit` (the pack the price is
        for, → derive **price-per-unit** for fair shop comparison), `observed_at`,
        `source` (bought / seen).
      - **Amount as DECIMAL(10,2) or integer minor-units — never float** (money
        must be exact; unlike `quantity`, which is DOUBLE).
      - Captured at the **buy→inventory** step (mark bought → optionally enter
        shop + paid). Derive: latest price, **cheapest shop**, price history,
        "where can I buy X", and an estimated Buy-list total.
      - Our observations are the source of truth; **don't trust OFF for price**
        (hyper-local/stale; Open Prices is at most a hint).
      - MVP: capture shop + amount at buy-time. Later: per-unit ranking,
        cheapest-shop, estimated totals, shop-trip scheduling via NC Calendar
        (overview §5).
- [ ] **Shopping list refinements** — add a recipe's missing ingredients to the
      Buy list in one tap; low-stock auto-suggestions. ~~Carry category through
      buy→inventory~~ DONE 2026-07-16 (shopping rows own category/product_id).
- [ ] **Recipe ingredients → product links** — a `recipe_ingredient.product_id`
      FK so ingredients resolve to catalog products instead of matching by name
      string ("cumin" vs "ground cumin" vs "cumin seeds" don't match today). This
      is the weakest joint in the data model; it unlocks reliable have-it? /
      missing-ingredient logic and the one-tap "add missing to Buy" above.
- [x] **Frontend: shared list-state component** — SHIPPED as increment A of
      `docs/proposals/wellbeing-timing-ux.md` (`<app-list-state>` used on all
      list screens). Remaining stragglers that bypass it (House error-as-empty,
      Today's hydration false-empty) are tracked as B2/D5 in the 2026-07-08
      review findings below.
- [ ] **Parsed net weight/volume → "how much is left at home"** — today the
      product's pack size is stored only as OFF's free-text `quantity_label`
      (e.g. `"950g"`), which is the right call *for now* (no parsing, no calc).
      Later, parse it into a numeric value + canonical unit so we can track
      **remaining amount** of an owned item (open a 950g tub, deduct as it's
      used) — and, as a side benefit, price-per-unit. Deferred until we actually
      want consumption tracking; keep storing the raw OFF label until then.
- [ ] **Whole-house inventory** — surface non-food categories in the UI (tools,
      documents, meds); the engine is already generic.
- [ ] **Meds / supplements** — expiry + refill-soon (fits the generic engine).
- [ ] **Warranties / receipts / manuals** — attach a file + purchase/expiry date.
- [ ] **Item history view** — the `item_history` audit is recorded but unshown.
- [ ] **House polish** — camera/lighting, per-cupboard layer visualisation,
      tap-a-cupboard-to-list-its-items.
- [x] **Offline support** — Angular service worker (ngsw, `registerImmediately`)
      prefetches the app shell AND caches read APIs (dataGroups, network-first):
      `/api/me`, items, locations, recipes, cookable, house, product images. App
      warm-fetches those on login so they're cached even for unvisited tabs. So
      the app opens with no signal and shows your inventory/recipes/house — the
      Tube case. Verified by `frontend/e2e/offline*.spec.ts` (npm run e2e) + on
      prod. Still online-only (writes/fresh data): editing.
- [ ] **PWA polish** — full icon set (png/maskable/favicon, not just svg).

## 2026-07-02 review findings (full list, priority-ordered)

From the six-agent review (backend, security, frontend, UX, data layer,
Android/infra). Batches already shipped: security quick fixes + WebView
hardening + SW update-on-visibility + lookup/buy feedback (A), restorable
deletion/trash (B), field-level sync merge + conflict log (C).

1. - [x] **TodoGraph stale catalogs** — items/recipes/places fetched once at
      injection; a just-added item can't be linked until a full reload.
2. - [x] **`depends_on` non-todo targets never block** — a to-do depending on
      an unbought shopping item / uncooked recipe shows "ready".
3. - [x] **Loading vs empty conflated** — lists flash "No items yet" on cold
      load before data arrives; needs loaded-state + progress indicator.
4. - [–] **Sha-tagged Docker images** — `:latest`-only means rollback is
      impossible; CI already has `github.sha`. **Decided NOT to do (2026-07-02):
      Pippijn rolls back via git revert + rebuild; images stay unversioned.**
5. - [x] **Non-root container + k8s securityContext** — app runs as root, no
      hardening context on app or DB pods.
6. - [–] **Frontend CI gate** — eslint/vitest/build run only in the local
      pre-push hook, not in CI (backend has `life-verify`). **Deferred
      (2026-07-02): not now, maybe later — the pre-push hook covers it for a solo
      dev.**
7. - [x] **Thumb-reachable Add** — top-anchored multi-field add forms → FAB +
      bottom sheet (Buy/To-do/Inventory/Recipes).
8. - [x] **Scanner: torch + manual entry** — no flashlight toggle, no "type it
      instead" fallback in the scanner dialog.
9. - [x] **Expiry urgency** — raw ISO dates; want "expired"/"3 days" coloring
      (ties into the Next-up expiry view).
10. - [x] **Search page** — REMOVED 2026-07-02. It was a name filter over
      items that duplicated the "All items" list; its only distinct payoff (→
      highlight in the 3D house) is parked. Home now lands on Inventory. Rebuild
      as part of the house-highlight feature if/when that lands, not as a tab.
11. - [x] **`todo_links` duplicate edges** — two offline devices adding the
      same connection both survive sync (client-only dedupe); dedupe on push +
      migration cleanup.
12. - [x] **Pin utf8mb4 charset/collation** — tables ride the server default;
      emoji/non-Latin correctness is luck.
13. - [x] **HTTP-layer router tests** — 401 paths, error mapping, body limits
      untested end-to-end (repos + pure fns are covered).
14. - [x] **Dedupe replication boilerplate + test guardAuth/migrations** — 3
      near-identical ~50-line blocks in the sync stores; auth-guard branches
      and RxDB migration strategies untested.
15. - [x] **Row-action consistency + tap targets** — three delete affordances
      across screens; dense to-do rows with sub-48px targets. *(2026-07-08
      review found the grammar drifted again — re-filed as B7 below.)*
16. - [~] **DB resource limits + NetworkPolicy** — limits + DB-ingress
      NetworkPolicy + securityContext SHIPPED 2026-07-02; the app-ingress
      policy is HELD (needs a kubelet-probe exemption on k3s first).
17. - [x] **Magic-byte image sniffing** — uploads/OFF fetches trust declared
      Content-Type (raster allowlist + nosniff/CSP already shipped; this is
      depth).
18. - [x] **Session sweeper** — expired session rows are only reaped lazily on
      re-presentation; abandoned ones accumulate.
19. - [~] **Polish basket** — DONE: `NC_BASE_URL` boot validation, LIKE-wildcard
      escaping. REMAINING: todo-detail title save on sheet dismiss; the
      "scenes/house.json" string in end-user copy; items sort/filter;
      `allowBackup=false` (needs the dev-lint canonical manifest updated too).
      (`setWebContentsDebuggingEnabled` was already done in `fbee581` — the entry
      outlived the work, and cost an hour of debugging on 2026-07-13 by making the
      WebView look un-inspectable. See android/README.md § Debugging the WebView.)

## 2026-07-08 review findings (priority-ordered)

From the three-part whole-codebase quality review (backend, frontend, UI
design) at commit `2815d45`. Full detail — failure scenarios, file refs, fix +
test per finding, batching — in `proposals/quality-review-2026-07-08.md`.
Verdict: A− across all three; the below-bar items are places where the right
pattern exists in the codebase and one spot didn't get it. Go given
2026-07-09; batches land in K → L → M → N order.

Below the bar (B), planned as batches K/L/M/N per the proposal:

1. - [x] **B1: wellbeing `emotions` masking fallback** — corrupt stored JSON
      silently pulls as "no emotions" fleet-wide (`src/sync/repo.rs:531`);
      read path must fail loudly like the write path.
2. - [x] **B2: House load failure rendered as empty** — `/api/house` error
      shows "No house layout yet." (`house.ts:48`); route through the shared
      error state.
3. - [x] **B3: TodoDetail dismiss-flush lacks the dirty guard** — remote edit
      arriving while the sheet is open is clobbered by the stale seed on
      dismiss; `wellbeing-entry`'s `noteDirty` fix applied to its sibling.
4. - [x] **B4: user-isolation tests** — core invariant, currently tested only
      in `conflicts_db.rs`; add two-user tests across sync pull/push, trash
      restore, REST lists.
5. - [x] **B5: sync push trusts the client** — `status='banana'` / `score=255`
      are stored, then 500 the whole list on read; validate at the push
      boundary, reject with 400 (reject, not clamp).
6. - [x] **B6: dedupe `src/sync/repo.rs`** — pull/push hand-copied 4× (637
      lines; set-only tombstone rule tested on 1 of 4 copies); factor over a
      per-collection spec, then test the tombstone rule per collection.
7. - [x] **B7: row-grammar drift + Buy FAB** — inventory's `more_vert` menu vs
      tap-to-edit+trailing-delete elsewhere; All-items has no delete; recipe
      delete misses `.danger`; the add-FAB disappears mid-shop once one item
      is checked. Unify rows; FAB and bought-bar share the bottom edge.

At the bar, do alongside (D, detail in the proposal): undo/restore helper
(D1 — done: the two-layer undo is now `SyncedStore.undoDelete`, and folding
wellbeing in fixed a latent bug where undoing a *synced* check-in delete
couldn't survive the server's set-only tombstone), `SyncedStore` base for the
near-identical stores (D2 — done: all four collections, incl. todo-link, now
extend it; the protocol/patch/remove/undo/replication spine is single-sourced),
specs for `todo-detail`/`buyDone`/geometry (D3 — done with batch N), harness coverage
for the ~half of pages uncovered + one dark-scheme golden (D4 — done with
batch N; it caught nothing real except the picker's sticky-footer occlusion,
scoped like the to-do sheet), Today's hydration false-empty
(D5 — done with batch K), mechanical consistency nits (D6 — done with batch M:
Space-key on `role="button"`, muted-text token vs opacity, one badge class,
shared `pathOf`; `.tappable` itself had drifted into three copies and was
consolidated global too).

~~Open decision for Pippijn: recipes edit path~~ — DECIDED and SHIPPED
2026-07-11: recipes got an edit path (backend update route + sheet edit
mode), matching the tap-to-edit grammar everywhere else.

## Open decisions

- three.js parametric geometry vs an authored glTF model of the house.
- How scene cupboards relate to the DB location tree (store `position` on the
  `location` rows, vs keep scene geometry separate and map by id/name).
- Whether barcode capture is worth the mobile-camera surface.
