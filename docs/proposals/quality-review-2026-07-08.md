# Proposal: 2026-07-08 quality review — findings & fix plan

Status: findings adjudicated (2026-07-08); go given 2026-07-09 — batches land
in K → L → M → N order (progress ticked in `../TODO.md`).
Checklist tracker: `../TODO.md` §"2026-07-08 review findings". This doc holds
the detail: what each finding is, why it is below (or at) the bar, the intended
fix, and the test that keeps it fixed.

Method: three parallel whole-codebase reviews at commit `2815d45` — Rust
backend (`src/`, `tests/`, `migrations/`), Angular frontend
(`frontend/src/app`, unit + e2e suites), and UI design judged from
templates/styles against the standing philosophy (standard Material grammar;
custom only where Material ships no component; `--mat-sys-*` tokens; minimal
custom CSS; everything correct at 412 px phone width). Key file:line claims
below were re-verified by hand against the working tree before filing.

## Verdict

All three areas grade **A−**. The deliberate engineering is at the level we
want: the sync protocol (commit-ordered revs, set-only tombstones, transactions
with reasoning written down; client-side `FieldSpec` merge making two past bug
classes unrepresentable), the security surface (SSRF allowlist, constant-time
session verify, tested against real attack strings), zoneless-signals
discipline, and the token/state/undo design grammar. What is **not** at level
is uniformity of application: each below-bar finding is a place where the right
pattern already exists in this codebase and one spot did not get it.

---

## 1. Below the bar — must fix

### B1. Masking fallback: corrupt wellbeing `emotions` silently becomes "none"

- **Where:** `src/sync/repo.rs:531-533` — `WellbeingDocRow → WellbeingDoc`
  parses the stored `emotions` JSON with
  `.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()`.
- **Failure:** a corrupt stored value is silently served as "no emotions" on
  every pull, propagating to all devices; the data loss is invisible. The
  write path a few lines below uses `.expect()` with a comment stating loud
  failure is the policy — the read path contradicts it, on wellbeing data.
- **Rule violated:** no masking fallbacks (fail loudly, don't paper over).
- **Fix:** propagate the parse error (500 on that pull) exactly like the write
  path; the row is then findable and repairable instead of shadow-broken.
- **Test:** `tests/wellbeing_db.rs` — seed a row with invalid `emotions` JSON;
  assert the pull fails rather than returning an empty list of emotions.

### B2. Masking fallback: House load failure rendered as empty state

- **Where:** `frontend/src/app/features/house/house.ts:48` —
  `error: () => this.empty.set(true)`; `house.html:5` then shows
  "No house layout yet."
- **Failure:** `/api/house` failing (offline, 500) is indistinguishable from a
  genuinely empty scene. Every other screen separates error from empty via the
  shared `<app-list-state>` grammar; House is the one page that conflates them
  (the exact defect 2026-07-02 finding 3 fixed everywhere else).
- **Fix:** classify the failure (`shared/api-error.ts`) and render the shared
  error+retry state; keep the empty state for a real 404/empty scene only.
- **Test:** new `house.spec.ts` — failing fetch → error state, not empty.

### B3. `TodoDetail` dismiss-flush lacks the dirty guard its sibling has

- **Where:** `frontend/src/app/features/todo/todo-detail.ts` (`ngOnDestroy`,
  ~lines 69-78): `title`/`notes` signals are seeded once from the doc; on
  destroy, any difference from the live doc is patched back.
- **Failure:** a remote edit (other device) landing while the sheet is open
  updates the live doc but not the seeded input signals; dismissing the sheet
  then patches the stale seed over the remote edit. This is the documented
  bug class that `wellbeing-entry.ts:48` already fixed with `noteDirty`
  ("…never the seeded original") — fixed in one sheet, still live in the other.
- **Fix:** same pattern: `titleDirty`/`notesDirty` set on user input; flush
  only dirty fields.
- **Test:** part of the new `todo-detail.spec.ts` (see D3) — remote patch
  while open, destroy without typing → no `patch` call.

### B4. User isolation is a core invariant with (almost) no tests

- **Where:** the only cross-user test in the suite is
  `tests/conflicts_db.rs` ("someone-else can't resolve"). Spot checks found no
  missing `user_id = ?` scoping in queries (the unscoped tables — `products`,
  `sync_rev` — are deliberately shared), but nothing would catch a regression,
  and the sync surface re-implements scoping per collection (see B6).
- **Rule violated:** the invariant every user's data is isolated is stated as
  core; core invariants get tests, not audits.
- **Fix (tests only, no product code expected):** a two-user fixture and,
  for each of the four sync collections plus trash + the REST lists:
  pull excludes the other user's docs; push onto another user's ulid cannot
  touch their row; trash restore is scoped. Document the known edge: ulids are
  globally unique per table, so a cross-user ulid collision on insert surfaces
  as a loud 500, not silent cross-writing — assert that too.

### B5. Sync push trusts the client; bad rows poison later reads

- **Where:** `src/sync/repo.rs` push paths. `WellbeingDoc.score`/`energy` are
  documented `1..5` (`src/sync/types.rs`) but any `u8` is stored;
  `TodoDoc.todo_type`/`status` and `TodoLinkDoc.kind` are stored as raw
  strings, parsed to enums only at the typed REST boundary (`src/todo/repo.rs`
  `TryFrom`).
- **Failure:** a push containing `status='banana'` is accepted, then the whole
  todo list read fails with a 500 — accepted at the boundary, exploding later,
  the inverse of fail-loudly. Same for out-of-range scores rendering as
  nonsense in the UI.
- **Fix:** validate at the push boundary and reject the offending doc with a
  400 (parse to the enum / range-check, then store). Decision: **reject, not
  clamp** — clamping would be another masking fallback.
- **Test:** per collection, push an invalid doc → 400 and nothing stored; a
  valid batch containing one invalid doc must not partially apply (the push is
  one transaction — assert that holds).

### B6. `src/sync/repo.rs` is a 4× hand-copy of the sync protocol

- **Where:** `pull_*` and `push_*` exist four times (shopping, todo,
  todo-link, wellbeing) — 637 lines that are one function schema-parameterised
  over (table, columns, row↔doc mapping). The protocol's safety rules
  (`FOR UPDATE` → rev guard → **set-only tombstone**) are re-implemented per
  copy, and the set-only rule is integration-tested only on the shopping copy
  (`tests/trash_db.rs::sync_push_cannot_resurrect_a_tombstone_but_restore_can`).
- **Why it matters:** a fifth collection means a fifth transcription of the
  protocol; drift in one copy ships silently (exactly the class the frontend's
  `FieldSpec` refactor made unrepresentable client-side — the server half has
  not caught up).
- **Fix:** factor pull/push over a small per-collection spec (trait or macro —
  implementer's choice; whichever reads simpler wins, per
  simplicity-over-cargo-cult). Target ≈250 lines. B4's isolation tests and
  B5's validation then apply to the *one* implementation; add one
  tombstone-resurrection test per collection (cheap once shared) to prove the
  copies converged.

### B7. UI: list-row action grammar drift + the Buy FAB vanishing mid-shop

Supersession note: 2026-07-02 finding 15 ("row-action consistency") shipped,
but the grammar has since drifted again — this re-files it with the current
shape.

- **Row grammar, per screen today:**
  - todo + shopping: tap-title-to-edit, trailing delete icon-button — the
    reference pattern;
  - inventory items (`inventory.html:33-41`): Edit/Delete behind a `more_vert`
    `mat-menu` — one extra tap for the same actions, different muscle memory;
  - All-items (`items.html:42`): tap-to-edit, **no delete affordance at all**;
  - recipes: `Delete` as a card text button without the `.danger` treatment
    every sheet's delete has, and **no edit path** (`recipe-sheet` is
    create-only).
- **Fix:** unify on tap-to-edit + trailing delete everywhere (undo makes
  direct delete safe — that was the point of universal Undo); `.danger` on the
  recipe delete. *Open decision (workflow, not style — needs Pippijn):*
  whether recipes gain an edit path (extend `recipe-sheet` to edit mode) or
  deliberately stay create+delete-only.
- **Buy FAB:** `shopping.html:46-58` — the add-FAB renders only while
  `doneCount() === 0`; the sticky "Got N → add to inventory" bar replaces it
  otherwise. During a real shop, checked items accumulate from the first
  minute, so the highest-frequency capture action ("add the thing I just
  remembered") is unavailable for most of the session. Fix: bar and FAB share
  the bottom edge (bar content left, FAB right — the bar is already sticky
  flex); no new chrome.
- **Test:** extend `ui-pages.spec.ts` busy-data coverage to the changed
  screens (see D4) and assert the FAB is present with items checked.

---

## 2. At the bar, tipping point — do alongside

Ranked; none is a rule violation, each is one copy short of forcing itself.

- **D1. Undo/restore triplication** — the revive-locally + `restoreTrash` +
  `reSync` + ignore-404 dance appears verbatim in `shopping.ts:75-91`,
  `todo.ts:161-181`, `todo-detail.ts:252-264`, subtle reasoning re-commented
  each time. Extract one store-level (or `Feedback`-level) helper.
- **D2. `SyncedStore` base** — `shopping-store.ts`/`todo-store.ts`/
  `wellbeing-store.ts` are ~70% identical (`items$` pipeline, `patch`/
  `remove`/`revive`/`reSync`/`find`/`init`/`startReplication`). 2026-07-02
  finding 14 deduped the replication wiring; the store shell is the remaining
  copy. Rule: **a fourth collection forces this refactor**; doing it with D1
  is cheaper than after.
- **D3. Missing specs where the logic is richest** — `todo-detail.ts` (the
  largest component: destroy-flush, group resolution, blocked-toggle guard,
  deferred link removal) has no spec; `shopping.ts` `buyDone()`
  partial-failure accounting likewise; `house.ts`/`scene-geometry.ts` are pure
  geometry, eminently unit-testable. B2/B3 land their tests here.
- **D4. Layout-harness coverage ≈ half the surface; dark scheme on faith** —
  `ui-pages.spec.ts` covers today/todo/wellbeing/buy/settings/todo-detail;
  uncovered: inventory, all-items, recipes, trash, conflicts, emotion picker
  (accordion + sticky footer — exactly the layout class that breaks silently),
  scanner, add-sheets. No spec runs in `prefers-color-scheme: dark`, so every
  `color-mix` tint's dark contrast is unverified. Add the overlap/overflow
  pair for the uncovered pages + one representative dark-scheme golden.
- **D5. Today's false-empty during hydration** — `today.html` shows "Nothing
  pressing right now." while stores hydrate; every other page shows the
  shared loading state. Route it through the same grammar.
- **D6. Small consistency debts (mechanical):**
  - `keydown.space` handled on `role="button"` spans in `todo-detail.html`
    but not `todo.html`/`shopping.html`/`today.html` (ARIA: Space must
    activate);
  - muted-text idiom split between `color: var(--mat-sys-on-surface-variant)`
    and bare `opacity: 0.6–0.8` (`trash.scss`, `items.scss`, `recipes.scss`,
    `todo-detail.scss`) — the token should win (opacity dims children and
    diverges in dark mode);
  - four hand-rolled count-badge pills (`app.scss` `.menu-badge`,
    `emotion-picker.scss` `.badge`, `today.scss` `.count`, plus `matBadge`)
    → one shared class;
  - `pathOf` breadcrumb walk duplicated in `inventory.ts:121-135` and
    `items.ts:104-117` → move next to `LocationsStore`.

---

## 3. Recorded, not planned

Filed so the adjudication isn't lost; none scheduled — each is either
documented-and-accepted or too small to batch. Revisit only when touching the
file anyway.

- `item_history.event` uses magic strings (`"added"`/`"moved"`/`"removed"`/
  `"restored"`) — the only string taxonomy left in `src/`; enum it when the
  item-history view (TODO backlog) lands.
- `inventory::repo` writes row + `record_history` without a transaction
  (unlike `create_recipe`); crash window loses only the audit row. Wrap when
  touched.
- Session TTL encoded twice (`src/session.rs` `SESSION_TTL_DAYS` and the
  cookie builder in `src/routes/auth.rs`) — drift risk, one-line unify.
- `routes/auth.rs` `callback` maps a missing `?code` to 500; should be 400.
- Hand-rolled `Display`/`FromStr` pairs duplicate serde renames (5 enums) —
  a serde round-trip helper would collapse both.
- `routes/products.rs:31` `fetch_image(...).ok().flatten()` drops the network
  `Err` unlogged (guard rejections inside do log) — add the log line.
- `restore_location` groups a delete-subtree by second-granularity
  `deleted_at`; two deletes in the same second merge on restore. Cosmetic.
- `AppError::NcNotLinked`/`NcReauthRequired` + `basic_auth_header` are
  pre-wired for CalDAV — speculative surface, kept deliberately (CalDAV is
  Next-up in `../TODO.md`).
- `house.ts:83` `NgZone.runOutsideAngular` is a no-op in a zoneless app;
  sheet refocus via `document.querySelector` instead of `viewChild` in two
  add-sheets; the 400 ms refresh-reveal gate implemented twice (cross-referenced
  in comments). All cosmetic.
- `.emo` emotion chips are `min-height: 40px` (guideline 48dp) with no
  hit-area extension in the picker grid; the emotion/timeline chips re-implement
  ~150 lines of what `mat-chip` gives free — the weakest (but argued) claims
  under the custom-CSS "earn it" rule. Reassess if they grow.

## 4. Batching

Ship order chosen so each batch is independently deployable and testable:

1. **Batch K — correctness / masking** (small diffs): B1, B2, B3 + their
   tests, D5, and the `fetch_image` log line.
2. **Batch L — sync boundary** (one backend PR): B6 dedup first, then B5
   validation and B4 isolation tests against the shared implementation;
   per-collection tombstone-resurrection tests close it out.
3. **Batch M — UI grammar** (frontend): B7 rows + FAB + `.danger`; D6
   mechanical consistency items ride along. Recipes edit path only if the
   open decision above resolves "yes".
4. **Batch N — harness**: D4 page coverage + dark-scheme golden; D3 specs
   (`todo-detail`, `buyDone`, geometry) land here or with the batch touching
   the file.
5. **D1 + D2 (store refactor)** — own batch after K so behavior is
   test-pinned before the extraction.

## 5. Open decisions

- Recipes: add an edit path, or deliberately create+delete-only? (Workflow
  semantics — Pippijn's call; everything else in B7 proceeds regardless.)
- B6 implementation shape: trait vs macro for the per-collection spec —
  implementer decides by which reads simpler; not a design decision.
