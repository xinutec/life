import { Injectable, inject, signal } from '@angular/core';
import { Observable, from } from 'rxjs';
import { map, shareReplay, switchMap } from 'rxjs/operators';
import { ulid } from 'ulid';
import { type RxCollection, type RxJsonSchema } from 'rxdb';

import { ConflictReporter, FieldSpec, makeConflictHandler } from './conflict-merge';
import { LifeDb } from './life-db';
import { startHttpReplication } from './replication';
import { SyncStatus } from './sync-status';

/** A wellbeing check-in as stored locally. `recordedAt` is an ISO-8601 UTC
 *  instant (the moment the feeling was — may be backdated); `score` is 1..5.
 *  Mirrors the backend `WellbeingDoc` wire shape. */
export interface WellbeingDoc {
  ulid: string;
  id: number | null;
  recordedAt: string;
  score: number;
  /** Optional energy reading (1..5, drained..energetic; higher = better, like
   *  `score`); null = mood-only. */
  energy: number | null;
  /** Fine-grained emotions from the feelings wheel (leaf words); independent of
   *  mood/energy, any number, order preserved as added. */
  emotions: string[];
  note: string | null;
  rev: number;
}

type WellbeingCollection = RxCollection<WellbeingDoc>;

const schema: RxJsonSchema<WellbeingDoc> = {
  // Bump the version + migrate on ANY schema change, else existing local DBs hit
  // a hash mismatch. v1: optional `fatigue`. v2: `emotions` array. v3: `fatigue`
  // → `energy` (unified polarity, higher = better).
  version: 3,
  primaryKey: 'ulid',
  type: 'object',
  properties: {
    ulid: { type: 'string', maxLength: 26 },
    id: { type: ['integer', 'null'] },
    recordedAt: { type: 'string', maxLength: 32 },
    score: { type: 'number', minimum: 1, maximum: 5 },
    energy: { type: ['number', 'null'], minimum: 1, maximum: 5 },
    emotions: { type: 'array', items: { type: 'string' } },
    note: { type: ['string', 'null'] },
    rev: { type: 'number' },
  },
  required: ['ulid', 'recordedAt', 'score', 'rev'],
};

/** A prior-version doc handed to a migration strategy — loosely typed, since the
 *  fields differ across versions (RxDB passes the old shape). */
type PriorDoc = Record<string, unknown> & {
  fatigue?: number | null;
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
};

/** The synced content fields (everything but the identity/server fields). */
type WellbeingContent = Omit<WellbeingDoc, 'ulid' | 'id' | 'rev'>;

/** Type-directed 3-way-merge spec: every content field with a strategy valid for
 *  its type (see [[makeConflictHandler]]). Exhaustive by construction — a field
 *  added to WellbeingDoc won't compile until it's classified here, and `emotions`
 *  (an array) can only be `'array'`, never identity-compared. */
const WELLBEING_FIELDS: FieldSpec<WellbeingContent> = {
  recordedAt: 'value',
  score: 'value',
  energy: 'value',
  emotions: 'array',
  note: 'value',
};

/** The field-name allowlist the Conflicts screen may patch on "use other",
 *  derived from the spec so the two can never drift apart. */
export const WELLBEING_MERGE_FIELDS = Object.keys(WELLBEING_FIELDS) as (keyof WellbeingContent)[];

/** Local-first store for wellbeing check-ins: the on-device RxDB collection is
 *  the source of truth; replication reconciles with /api/sync/wellbeing in the
 *  background. Reads are reactive and offline; writes are local + optimistic. */
@Injectable({ providedIn: 'root' })
export class WellbeingStore {
  /** null = ok; a string = a sync problem to surface. */
  readonly syncError = signal<string | null>(null);

  private lifeDb = inject(LifeDb);
  private reporter = inject(ConflictReporter);
  private syncStatus = inject(SyncStatus);
  private readonly collection = this.init();
  private replication?: ReturnType<typeof startHttpReplication<WellbeingDoc>>;

  /** Live, non-deleted check-ins, newest first. */
  readonly items$: Observable<WellbeingDoc[]> = from(this.collection).pipe(
    switchMap((col) => col.find({ sort: [{ recordedAt: 'desc' }] }).$),
    map((docs) => docs.map((d) => d.toJSON() as WellbeingDoc)),
    shareReplay({ bufferSize: 1, refCount: false }),
  );

  /** Insert a check-in; returns its minted ulid (so a caller can offer Undo). */
  async add(input: { recordedAt: string; score: number; note: string | null }): Promise<string> {
    const col = await this.collection;
    const key = ulid();
    await col.insert({
      ulid: key,
      id: null,
      recordedAt: input.recordedAt,
      score: input.score,
      energy: null,
      emotions: [],
      note: input.note,
      rev: 0,
    });
    return key;
  }

  async patch(key: string, fields: Partial<WellbeingContent>): Promise<void> {
    const doc = await this.find(key);
    await doc?.incrementalPatch(fields);
  }

  async remove(key: string): Promise<void> {
    const doc = await this.find(key);
    await doc?.remove();
  }

  /** Bring a just-removed doc back locally under the same ulid (the Undo
   *  snackbar's first layer; server-side trash restore is the second). */
  async revive(doc: WellbeingDoc): Promise<void> {
    const col = await this.collection;
    await col.insert({ ...doc });
  }

  /** Pull now — e.g. right after a server-side trash restore. */
  reSync(): void {
    this.replication?.reSync();
  }

  private async find(key: string) {
    const col = await this.collection;
    return col.findOne(key).exec();
  }

  private async init(): Promise<WellbeingCollection> {
    const handler = makeConflictHandler<WellbeingDoc>({
      fields: WELLBEING_FIELDS,
      onConflicts: (kept, conflicts) =>
        this.reporter.report('wellbeing', kept.ulid, `Check-in (${kept.score}/5)`, conflicts),
    });
    const col = await this.lifeDb.collection('wellbeing', schema, handler, migrationStrategies);
    this.startReplication(col);
    return col;
  }

  private startReplication(collection: WellbeingCollection): void {
    this.replication = startHttpReplication<WellbeingDoc>({
      collection,
      // '-v2': replication-state reset (2026-07-03). The isEqual push-loss bug
      // (conflict-merge.ts) advanced the push checkpoint past field edits
      // without sending them; a fresh identifier makes RxDB re-examine every
      // doc on next start so stranded edits finally push. Local state wins —
      // downstream defers to upstream for forks without meta (rxdb#7804).
      identifier: 'wellbeing-http-sync-v2',
      path: '/api/sync/wellbeing',
      syncError: this.syncError,
      syncStatus: this.syncStatus,
      label: 'wellbeing sync',
    });
  }
}
