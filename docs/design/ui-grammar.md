# life — interaction grammar

One grammar applied everywhere, so a screen is never improvised. When a screen
disagrees with this file, the screen is the bug.

**Material first.** Standard Angular Material components, `--mat-sys-*` tokens,
minimal custom CSS. Custom is for the cases where Material ships nothing (the
bottom-tab / side-rail nav is the standing example). Everything must be correct
at **412 px** — the phone is the primary target, not a narrow edge case.

## The rules

| Action | Standard |
|---|---|
| Add | FAB → **bottom sheet** with the form |
| Edit | the same sheet, pre-filled — add and edit are literally one component |
| Row | **tap the title to edit, trailing icon to delete** — on every list |
| Delete | always an **Undo snackbar**; never an unrecoverable in-place delete |
| Load failure | `<app-list-state>` with a Retry button |
| Action failure | `Feedback.error` snackbar — no raw `MatSnackBar` in features |
| Status markers | `mat-chip` styling via the shared classes |
| Form fields | `appearance="outline"` + `subscriptSizing="dynamic"` |

Two shared pieces carry most of it, both in `frontend/src/app/shared/`:
`<app-list-state>` (the loading / empty / error triad as one component) and
`Feedback` (`error()`, and `undo()` which commits on dismiss).

Errors have exactly two channels and each has one job: a **load** failure renders
in place with Retry, an **action** failure is a snackbar. Never conflate an error
with an empty state — "no house layout yet" for a failed fetch is the bug this
rule exists to prevent.

## Small things that keep drifting back

- Muted text is `color: var(--mat-sys-on-surface-variant)`, **not** `opacity`.
  Opacity dims children too and diverges in dark mode.
- `.count-badge` is the badge. A screen adds placement on top of it and nothing
  else — hand-rolled count pills have been consolidated once already.
- `keydown.space` must activate anything with `role="button"` — Space is an ARIA
  requirement, not a nicety.
- Fonts and icons are **self-hosted**; a CDN font is a blank icon on an offline
  launch.

## Verification

`frontend/e2e/ui-pages.spec.ts` asserts no text overlap and no horizontal
overflow across the screens at phone width, and `ui-golden.spec.ts` carries a
dark-scheme golden. Layout is judged from the render, not from reading the
template. Keep the overflow oracle's `allow` list narrow — every entry is an
element it stops measuring.

## Non-goals

Notifications and reminders (NC Calendar owns them), recurrence, a charting
library (hand-written SVG until charts multiply), a theme toggle (follow the OS),
multi-user anything.
