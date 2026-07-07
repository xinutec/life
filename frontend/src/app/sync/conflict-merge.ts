import { Injectable, inject } from '@angular/core';
import { MatSnackBar } from '@angular/material/snack-bar';
import { Router } from '@angular/router';
import type { RxConflictHandler } from 'rxdb';

import { Alerts } from '../shared/alerts';
import { LifeApi } from '../life-api';
import { ConflictKind } from '../models';

/** One same-field collision the merge had to decide: `mine` was kept (the
 *  pushing device's latest intent), `theirs` lost and gets logged. */
export interface FieldConflict {
  field: string;
  mine: unknown;
  theirs: unknown;
}

/** A per-document record of what `resolve()` actually did — the merge path is
 *  otherwise invisible (the isEqual push-loss bug went undetected precisely
 *  because nothing here logs). `mine`/`theirs`/`collided` name the fields that
 *  resolved each way, so a stray edit "disturbed" by a merge leaves a trace. */
export interface MergeTrace {
  ulid: string;
  /** Fields taken from this device (I changed them since the assumed base). */
  mine: string[];
  /** Fields left to the server value that the OTHER device changed — the branch
   *  that pulls in remote edits; the one to watch for clobbered local work. */
  theirs: string[];
  /** Fields both sides changed to different values: local won, server logged. */
  collided: string[];
  /** The whole doc resolved to a tombstone (a delete won). */
  deleted: boolean;
  /** No assumed base to diff against → the local doc won wholesale. */
  noBase: boolean;
}

/** How a field's two values are judged equal. The set of strategies a field may
 *  declare is TYPE-DIRECTED (see [[FieldSpec]] / [[EqFor]]), so a spec cannot
 *  misclassify a field — an array can never be compared by identity by accident. */
export type FieldEq = 'value' | 'array';

/** The equality strategies valid for a field of type `V`:
 *  - an array field → `'array'` (element-wise; identity reads a fresh-but-equal
 *    copy as changed — the emotions sync bug);
 *  - a primitive / nullable-primitive → `'value'` (identity, undefined ≡ null).
 *  A non-array OBJECT field resolves to `never` on purpose: there's no safe
 *  strategy for one yet, so introducing such a field is a COMPILE error until
 *  `FieldEq` gains a deep comparer — it can never silently fall back to identity
 *  (the very trap that made array identity a bug). */
type EqFor<V> =
  NonNullable<V> extends readonly unknown[] ? 'array' : NonNullable<V> extends object ? never : 'value';

/** An exhaustive, type-directed 3-way-merge spec: every content field of `C`,
 *  each tagged with a strategy valid for its type. `-?` makes every key required,
 *  so a field added to the document won't compile until it's classified here — it
 *  can never be silently dropped from the merge (the push-loss bug) nor wrongly
 *  identity-compared (the emotions bug). */
export type FieldSpec<C> = { [K in keyof C]-?: EqFor<C[K]> };

/** Compare one field's two values under its declared strategy. `undefined` folds
 *  to `null`, so an absent optional equals the wire's explicit null. */
function eqBy(strategy: FieldEq, a: unknown, b: unknown): boolean {
  const x = a ?? null;
  const y = b ?? null;
  if (strategy === 'array') {
    if (!Array.isArray(x) || !Array.isArray(y)) return Object.is(x, y);
    return x.length === y.length && x.every((v, i) => Object.is(v, y[i]));
  }
  return Object.is(x, y);
}

/** Default merge observer: DevTools "Verbose"-level only, so it's silent in
 *  normal use but readable over CDP when diagnosing a sync. */
function logMergeTrace(t: MergeTrace): void {
  console.debug('[conflict:resolve]', t.ulid, {
    mine: t.mine,
    theirs: t.theirs,
    collided: t.collided,
    ...(t.deleted ? { deleted: true } : {}),
    ...(t.noBase ? { noBase: true } : {}),
  });
}

/** Build a field-level 3-way-merge conflict handler for a synced collection.
 *
 *  A conflict means the same row changed on two devices while one was offline.
 *  The old whole-document rule (local wins) silently dropped the other
 *  device's edits even on fields this device never touched. Instead, diff
 *  against the assumed base (the state this device last synced):
 *
 *  - a field only I changed → mine;
 *  - a field only they changed → theirs — nothing of theirs is lost anymore;
 *  - a field we BOTH changed → mine (the user pushing is the latest intent),
 *    and the losing value is handed to `onConflicts` for the conflict log —
 *    decided, but never silently discarded.
 *
 *  Deletes: a server tombstone stands (the server is set-only — a push can't
 *  clear it; the trash restore is the one undelete), and a local delete stands
 *  over remote edits. Identity/server fields (ulid, id, rev) always come from
 *  the real master. */
export function makeConflictHandler<
  T extends { rev: number },
  C = Omit<T, 'ulid' | 'id' | 'rev'>,
>(opts: {
  /** Type-directed, exhaustive: every content field of `C` with a strategy valid
   *  for its type. Its keys ARE the field set the merge diffs (see [[FieldSpec]]). */
  fields: FieldSpec<C>;
  onConflicts?: (kept: T & { _deleted: boolean }, conflicts: FieldConflict[]) => void;
  /** Observe every resolve() decision. Defaults to a `console.debug` trace;
   *  a test injects a spy to assert the merge disturbed no local edit. */
  trace?: (t: MergeTrace) => void;
}): RxConflictHandler<T> {
  const trace = opts.trace ?? logMergeTrace;
  // The spec's keys are the merge field set; each maps to its equality strategy.
  const spec = opts.fields as Record<string, FieldEq>;
  const keys = Object.keys(spec);
  const get = (o: unknown, f: string): unknown => (o as Record<string, unknown>)[f];
  const set = (o: unknown, f: string, v: unknown): void => {
    (o as Record<string, unknown>)[f] = v;
  };
  const eq = (f: string, a: unknown, b: unknown): boolean => eqBy(spec[f], a, b);
  return {
    /** Replication equality. RxDB asks this in BOTH directions, and the
     *  upstream one is load-bearing: `isEqual(assumedMaster, current,
     *  'upstream-check-if-equal')` decides whether a local doc still needs
     *  pushing — `false` is what queues the push. Revs are server-minted, so
     *  a local edit changes content but NOT `rev`; comparing rev alone judged
     *  every field edit "already replicated" and silently dropped it (the
     *  2026-07-03 push-loss bug — see replication-push.spec.ts). The content
     *  fields must be compared too — under each field's declared strategy, so
     *  array fields (emotions) don't read as forever-changed (see [[eqBy]]). */
    isEqual: (a, b) =>
      !!a._deleted === !!b._deleted &&
      (!!a._deleted || (a.rev === b.rev && keys.every((f) => eq(f, get(a, f), get(b, f))))),
    resolve: ({ realMasterState: real, newDocumentState: mine, assumedMasterState: assumed }) => {
      const id = (mine as { ulid?: string }).ulid ?? '?';
      if (real._deleted) {
        trace({ ulid: id, mine: [], theirs: [], collided: [], deleted: true, noBase: false });
        return Promise.resolve(real);
      }
      if (!assumed) {
        // No base to diff against → the local doc wins wholesale.
        trace({ ulid: id, mine: [], theirs: [], collided: [], deleted: !!mine._deleted, noBase: true });
        return Promise.resolve(mine);
      }
      const merged = { ...real };
      const conflicts: FieldConflict[] = [];
      const tookMine: string[] = [];
      const tookTheirs: string[] = [];
      for (const f of keys) {
        if (eq(f, get(mine, f), get(assumed, f))) {
          // I didn't touch it → keep the master's value; note when that pulls
          // in a genuine remote change (real ≠ base), not just an unchanged field.
          if (!eq(f, get(real, f), get(assumed, f))) tookTheirs.push(f);
          continue;
        }
        if (!eq(f, get(real, f), get(assumed, f)) && !eq(f, get(mine, f), get(real, f))) {
          conflicts.push({ field: f, mine: get(mine, f), theirs: get(real, f) });
        }
        set(merged, f, get(mine, f));
        tookMine.push(f);
      }
      if (mine._deleted) merged._deleted = true;
      trace({
        ulid: id,
        mine: tookMine,
        theirs: tookTheirs,
        collided: conflicts.map((c) => c.field),
        deleted: !!mine._deleted,
        noBase: false,
      });
      if (conflicts.length > 0) opts.onConflicts?.(mine, conflicts);
      return Promise.resolve(merged);
    },
  };
}

/** Sends same-field conflicts to the server-side conflict log (so every device
 *  can review them) and points the user at the Conflicts screen. */
@Injectable({ providedIn: 'root' })
export class ConflictReporter {
  private api = inject(LifeApi);
  private snack = inject(MatSnackBar);
  private router = inject(Router);
  private alerts = inject(Alerts);

  report(kind: ConflictKind, ulid: string, label: string, conflicts: FieldConflict[]): void {
    for (const c of conflicts) {
      this.api
        .reportConflict({
          kind,
          ulid,
          field: c.field,
          label,
          mine: JSON.stringify(c.mine ?? null),
          theirs: JSON.stringify(c.theirs ?? null),
        })
        .subscribe({
          // The merge already happened; a failed report only loses the log
          // entry. Warn instead of interrupting sync.
          error: () => console.warn('[conflict] report failed', kind, ulid, c.field),
        });
    }
    this.alerts.addConflicts(conflicts.length); // badge appears immediately
    this.snack
      .open(`Edits collided on “${label}” — kept this device's version.`, 'Review', {
        duration: 8000,
      })
      .onAction()
      .subscribe(() => void this.router.navigate(['/conflicts']));
  }
}
