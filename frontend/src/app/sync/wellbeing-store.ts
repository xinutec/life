import { Injectable, inject } from '@angular/core';
import { ulid } from 'ulid';
import { type RxJsonSchema } from 'rxdb';

import { ConflictReporter, FieldSpec, makeConflictHandler } from './conflict-merge';
import { SyncedCollectionConfig, SyncedStore } from './synced-store';

/** A wellbeing check-in as stored locally. `recordedAt` is an ISO-8601 UTC
 *  instant (the moment the feeling was — may be backdated). Readings are in TENTHS
 *  of a point (10..50), so a 3.5 — a mood between two faces — is a 35, and readings
 *  stay exact integers under averaging. Mirrors the backend `WellbeingDoc`. */
// dev-lint: allow-wire-mirror RxDB owns the _deleted tombstone dimension;
// the wire type adds it in the replication layer, not in this local doc.
export interface WellbeingDoc {
  ulid: string;
  id: number | null;
  recordedAt: string;
  scoreTenths: number;
  /** Optional energy reading (10..50 tenths, drained..energetic; higher = better,
   *  like the score); null = mood-only. */
  energyTenths: number | null;
  /** Fine-grained emotions from the feelings wheel (leaf words); independent of
   *  mood/energy, any number, order preserved as added. */
  emotions: string[];
  note: string | null;
  rev: number;
}

const schema: RxJsonSchema<WellbeingDoc> = {
  // Bump the version + migrate on ANY schema change, else existing local DBs hit
  // a hash mismatch. v1: optional `fatigue`. v2: `emotions` array. v3: `fatigue`
  // → `energy` (unified polarity, higher = better). v4: score/energy → tenths.
  version: 4,
  primaryKey: 'ulid',
  type: 'object',
  properties: {
    ulid: { type: 'string', maxLength: 26 },
    id: { type: ['integer', 'null'] },
    recordedAt: { type: 'string', maxLength: 32 },
    scoreTenths: { type: 'number', minimum: 10, maximum: 50 },
    energyTenths: { type: ['number', 'null'], minimum: 10, maximum: 50 },
    emotions: { type: 'array', items: { type: 'string' } },
    note: { type: ['string', 'null'] },
    rev: { type: 'number' },
  },
  required: ['ulid', 'recordedAt', 'scoreTenths', 'rev'],
};

/** A prior-version doc handed to a migration strategy — loosely typed, since the
 *  fields differ across versions (RxDB passes the old shape). */
type PriorDoc = Record<string, unknown> & {
  fatigue?: number | null;
  score?: number;
  energy?: number | null;
  emotions?: string[];
};

// Exported for wellbeing-store.spec.ts — a stale local DB (an old browser
// profile, the Android WebView) runs these once on next open, so pin them.
export const migrationStrategies = {
  1: (doc: PriorDoc): PriorDoc => ({ ...doc, fatigue: doc.fatigue ?? null }),
  2: (doc: PriorDoc): PriorDoc => ({ ...doc, emotions: doc.emotions ?? [] }),
  // v3: fatigue (1=none..5=severe, higher=worse) → energy (its complement,
  // higher=better) so nothing runs inverse in the data. `6 - fatigue`.
  3: ({ fatigue, ...rest }: PriorDoc): PriorDoc => ({
    ...rest,
    energy: fatigue == null ? null : 6 - fatigue,
  }),
  // v4: 1..5 points → 10..50 tenths, so a mood can sit between two faces. The
  // fields are RENAMED as well as rescaled: a leftover `score: 4` reaching code
  // that now means tenths would read as a 0.4 — the worst day ever logged. Gone
  // from the shape, it can't be read at all.
  4: ({ score, energy, ...rest }: PriorDoc): PriorDoc => ({
    ...rest,
    scoreTenths: (score ?? 3) * 10,
    energyTenths: energy == null ? null : energy * 10,
  }),
};

/** The synced content fields (everything but the identity/server fields). */
type WellbeingContent = Omit<WellbeingDoc, 'ulid' | 'id' | 'rev'>;

/** Type-directed 3-way-merge spec: every content field with a strategy valid for
 *  its type (see [[makeConflictHandler]]). Exhaustive by construction — a field
 *  added to WellbeingDoc won't compile until it's classified here, and `emotions`
 *  (an array) can only be `'array'`, never identity-compared. */
const WELLBEING_FIELDS: FieldSpec<WellbeingContent> = {
  recordedAt: 'value',
  scoreTenths: 'value',
  energyTenths: 'value',
  emotions: 'array',
  note: 'value',
};

/** The field-name allowlist the Conflicts screen may patch on "use other",
 *  derived from the spec so the two can never drift apart. */
export const WELLBEING_MERGE_FIELDS = Object.keys(WELLBEING_FIELDS) as (keyof WellbeingContent)[];

/** Local-first store for wellbeing check-ins — the machinery lives in
 *  {@link SyncedStore}; this declares only the collection and its content. */
@Injectable({ providedIn: 'root' })
export class WellbeingStore extends SyncedStore<WellbeingDoc> {
  private reporter = inject(ConflictReporter);

  /** Live, non-deleted check-ins, newest first. */
  readonly items$ = this.liveQuery([{ recordedAt: 'desc' }]);

  protected config(): SyncedCollectionConfig<WellbeingDoc> {
    return {
      name: 'wellbeing',
      schema,
      conflictHandler: makeConflictHandler<WellbeingDoc>({
        fields: WELLBEING_FIELDS,
        onConflicts: (kept, conflicts) =>
          this.reporter.report(
            'wellbeing',
            kept.ulid,
            `Check-in (${kept.scoreTenths / 10}/5)`,
            conflicts,
          ),
      }),
      // '-v2': replication-state reset (2026-07-03). The isEqual push-loss bug
      // (conflict-merge.ts) advanced the push checkpoint past field edits
      // without sending them; a fresh identifier makes RxDB re-examine every
      // doc on next start so stranded edits finally push. Local state wins —
      // downstream defers to upstream for forks without meta (rxdb#7804).
      identifier: 'wellbeing-http-sync-v2',
      path: '/api/sync/wellbeing',
      label: 'wellbeing sync',
      trashKind: 'wellbeing',
      migrationStrategies,
    };
  }

  /** Insert a check-in; returns its minted ulid (so a caller can offer Undo). */
  async add(input: {
    recordedAt: string;
    scoreTenths: number;
    note: string | null;
  }): Promise<string> {
    const col = await this.collection;
    const key = ulid();
    await col.insert({
      ulid: key,
      id: null,
      recordedAt: input.recordedAt,
      scoreTenths: input.scoreTenths,
      energyTenths: null,
      emotions: [],
      note: input.note,
      rev: 0,
    });
    return key;
  }
}
