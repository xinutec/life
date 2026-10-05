import { Injectable, inject } from '@angular/core';
import { MatSnackBar } from '@angular/material/snack-bar';
import { Router } from '@angular/router';
import type { RxConflictHandler } from 'rxdb';

import { Alerts } from '../shared/alerts';
import { LifeApi } from '../life-api';
import { ConflictKind } from '../models';
import { isRecord, stringField } from '../shared/narrow';

/** A same-field collision: `mine` was kept, `theirs` is logged. */
export interface FieldConflict {
  field: string;
  mine: unknown;
  theirs: unknown;
}

/** What `resolve()` did to one document, field by field, so a merge that
 *  disturbs a local edit leaves a trace. */
export interface MergeTrace {
  ulid: string;
  /** Changed here since the base: taken from this device. */
  mine: string[];
  /** Changed only by the other device: taken from the server. */
  theirs: string[];
  /** Changed on both: this device won, the server's value is logged. */
  collided: string[];
  deleted: boolean;
  /** No base to diff against, so the local doc won whole. */
  noBase: boolean;
}

/** How a field's two values are judged equal. */
export type FieldEq = 'value' | 'array';

/** The strategy a field of type `V` must use. An object field is `never`, so
 *  it fails to compile rather than fall back to identity. */
type EqFor<V> =
  NonNullable<V> extends readonly unknown[]
    ? 'array'
    : NonNullable<V> extends object
      ? never
      : 'value';

/** Every content field of `C` with its strategy. Required keys, so a new field
 *  cannot compile until the merge knows it. */
export type FieldSpec<C> = { [K in keyof C]-?: EqFor<C[K]> };

/** `undefined` equals `null`: an absent optional is the wire's null. */
function eqBy(strategy: FieldEq, a: unknown, b: unknown): boolean {
  const x = a ?? null;
  const y = b ?? null;
  if (strategy === 'array') {
    if (!Array.isArray(x) || !Array.isArray(y)) return Object.is(x, y);
    return x.length === y.length && x.every((v, i) => Object.is(v, y[i]));
  }
  return Object.is(x, y);
}

/** Verbose-level, so it is silent unless someone is reading over CDP. */
function logMergeTrace(t: MergeTrace): void {
  console.debug('[conflict:resolve]', t.ulid, {
    mine: t.mine,
    theirs: t.theirs,
    collided: t.collided,
    ...(t.deleted ? { deleted: true } : {}),
    ...(t.noBase ? { noBase: true } : {}),
  });
}

/** A field-level 3-way merge against the base this device last synced: a field
 *  changed on one side takes that side; on both, this device wins and the loser
 *  goes to `onConflicts`. A server tombstone stands; a local delete beats
 *  remote edits. */
export function makeConflictHandler<
  T extends { rev: number },
  C = Omit<T, 'ulid' | 'id' | 'rev'>,
>(opts: {
  fields: FieldSpec<C>;
  onConflicts: (kept: T & { _deleted: boolean }, conflicts: FieldConflict[]) => void;
  trace?: (t: MergeTrace) => void;
}): RxConflictHandler<T> {
  const trace = opts.trace ?? logMergeTrace;
  const spec = opts.fields as Record<string, FieldEq>;
  const keys = Object.keys(spec);
  const get = (o: unknown, f: string): unknown => (isRecord(o) ? o[f] : undefined);
  const set = (o: unknown, f: string, v: unknown): void => {
    if (isRecord(o)) o[f] = v;
  };
  const eq = (f: string, a: unknown, b: unknown): boolean => eqBy(spec[f], a, b);
  return {
    /** Also decides whether a local doc still needs pushing; an edit leaves
     *  `rev` alone, so the fields must be compared too. */
    isEqual: (a, b) =>
      !!a._deleted === !!b._deleted &&
      (!!a._deleted || (a.rev === b.rev && keys.every((f) => eq(f, get(a, f), get(b, f))))),
    resolve: ({ realMasterState: real, newDocumentState: mine, assumedMasterState: assumed }) => {
      const id = stringField(mine, 'ulid') ?? '?';
      if (real._deleted) {
        trace({ ulid: id, mine: [], theirs: [], collided: [], deleted: true, noBase: false });
        return Promise.resolve(real);
      }
      if (!assumed) {
        trace({
          ulid: id,
          mine: [],
          theirs: [],
          collided: [],
          deleted: !!mine._deleted,
          noBase: true,
        });
        return Promise.resolve(mine);
      }
      const merged = { ...real };
      const conflicts: FieldConflict[] = [];
      const tookMine: string[] = [];
      const tookTheirs: string[] = [];
      for (const f of keys) {
        if (eq(f, get(mine, f), get(assumed, f))) {
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
      if (conflicts.length > 0) opts.onConflicts(mine, conflicts);
      return Promise.resolve(merged);
    },
  };
}

/** Logs collisions on the server, where every device can review them. */
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
          error: () => console.warn('[conflict] report failed', kind, ulid, c.field),
        });
    }
    this.alerts.addConflicts(conflicts.length);
    this.snack
      .open(`Edits collided on “${label}” — kept this device's version.`, 'Review', {
        duration: 8000,
      })
      .onAction()
      .subscribe(() => void this.router.navigate(['/conflicts']));
  }
}
