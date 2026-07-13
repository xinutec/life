# Life — roadmap & TODO

Living checklist for the Life app. Keep it current: tick items as they ship,
add new ones under the right section. Architecture/rationale lives in
`docs/design/overview.md`; this is the "what's done / what's next" tracker.

## Known bug: signing in from the app fails when logged OUT of Nextcloud

- [ ] **Nextcloud login-flow loses its session on the cross-site hop.** Pressing
  "Sign in with Nextcloud" in the WebView fails with Nextcloud's *"Access denied —
  State token does not match"* whenever the WebView holds **no Nextcloud session**.
  Sign in to Nextcloud first and it works, which is why it has gone unnoticed: the
  WebView normally has a session. It will bite on a fresh install or after clearing
  cookies (2026-07-13: four failed attempts, no callback ever reached us).

  Mechanism, from `apps/oauth2/lib/Controller/LoginRedirectorController.php` in the
  running Nextcloud (33.0.2, oauth2 1.21.0): a logged-out `authorize` puts
  `oauth.state` in the PHP session, redirects to `core.ClientFlowLogin`'s
  auth-picker, which puts a second secret (`stateToken`) in that same session; the
  later `grantPage` compares them. So the login needs **one Nextcloud session to
  survive three hops** — and the chain *begins with a cross-site navigation*
  (life.xinutec.org → dash.xinutec.org), on which the browser deliberately withholds
  Nextcloud's `__Host-nc_sameSiteCookiestrict` (SameSite=strict) cookie. Something
  in there drops the session; which hop is not yet pinned.

  NOT yet established: the exact hop. Nextcloud logs nothing (the refusal is a plain
  403, not an exception) and the chain is self-consistent when walked with curl and a
  cookie jar — so the divergence is in the WebView's cookie handling. Note
  `MainActivity` sets `setAcceptThirdPartyCookies` on the hidden and connect
  WebViews but NOT on the main one, and never calls `CookieManager.flush()`; both are
  suspects, neither is confirmed.

  To finish it: the WebView is remote-debuggable (see android/README.md § Debugging
  the WebView) — sign out of Nextcloud in the app, attach CDP, capture the Network
  events across the failing chain, and find the request where the session cookie goes
  missing. That costs a Nextcloud sign-out, so do it deliberately, not in passing.

## Backup — deliberately NOT yet (wait for Pippijn's go)

- [ ] **Back up the Life DB** — **DO NOT set this up yet.** It matters *after*
  the system is developed and the DB schema is **stable**; while migrations are
  still being added, a backup is premature. Pippijn will say when to start —
  don't start, offer to start, or flag it as overdue before then.
  When the time comes: scheduled `mysqldump` of the `life` DB on isis (ns
  `life`, deploy `life-db`, PVC `life-db-pvc`) folded into the Mac-mini
  **restic** set (`xinutec-infra/mac-mini/hm-agents.nix`, daily 05:00). Until
  then the PVC is the only copy — that's an accepted, temporary state. (overview §6)

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
      Buy list in one tap; low-stock auto-suggestions; carry category through
      buy→inventory (currently defaults to `other`).
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

Open decision for Pippijn: recipes edit path — add one, or deliberately
create+delete-only? B7 shipped with recipes create+delete-only (the delete
got `.danger`); an edit path needs a backend update route + sheet edit mode
once decided.

## Open decisions

- three.js parametric geometry vs an authored glTF model of the house.
- How scene cupboards relate to the DB location tree (store `position` on the
  `location` rows, vs keep scene geometry separate and map by id/name).
- Whether barcode capture is worth the mobile-camera surface.
