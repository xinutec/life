import { Injectable, inject } from '@angular/core';
import { ulid } from 'ulid';
import { type RxJsonSchema } from 'rxdb';

import { ConflictReporter, FieldSpec, makeConflictHandler } from './conflict-merge';
import { SyncedCollectionConfig, SyncedStore } from './synced-store';
import { keysOf } from '../shared/narrow';

/** A check-in as stored locally; mirrors the backend `WellbeingDoc`. Readings
 *  are in tenths (10..50), so a 3.5 is an exact 35. */
export interface WellbeingDoc {
  ulid: string;
  id: number | null;
  recordedAt: string;
  scoreTenths: number;
  /** Null for a mood-only check-in. */
  energyTenths: number | null;
  /** Feelings-wheel tokens, in the order added. */
  emotions: string[];
  note: string | null;
  rev: number;
}

const schema: RxJsonSchema<WellbeingDoc> = {
  // Any schema change needs a version bump and a migration.
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

/** A doc of some earlier version, as a migration receives it. */
type PriorDoc = Record<string, unknown> & {
  fatigue?: number | null;
  score?: number;
  energy?: number | null;
  emotions?: string[];
};

// Exported so the spec can pin them: an old device runs them on next open.
export const migrationStrategies = {
  1: (doc: PriorDoc): PriorDoc => ({ ...doc, fatigue: doc.fatigue ?? null }),
  2: (doc: PriorDoc): PriorDoc => ({ ...doc, emotions: doc.emotions ?? [] }),
  // fatigue (higher = worse) becomes energy (higher = better).
  3: ({ fatigue, ...rest }: PriorDoc): PriorDoc => ({
    ...rest,
    energy: fatigue == null ? null : 6 - fatigue,
  }),
  // Points to tenths, renamed so a leftover `score: 4` cannot be read as 0.4.
  4: ({ score, energy, ...rest }: PriorDoc): PriorDoc => ({
    ...rest,
    scoreTenths: (score ?? 3) * 10,
    energyTenths: energy == null ? null : energy * 10,
  }),
};

type WellbeingContent = Omit<WellbeingDoc, 'ulid' | 'id' | 'rev'>;

const WELLBEING_FIELDS: FieldSpec<WellbeingContent> = {
  recordedAt: 'value',
  scoreTenths: 'value',
  energyTenths: 'value',
  emotions: 'array',
  note: 'value',
};

/** The fields "use other" may patch on the Conflicts screen. */
export const WELLBEING_MERGE_FIELDS = keysOf(WELLBEING_FIELDS);

/** Wellbeing check-ins. */
@Injectable({ providedIn: 'root' })
export class WellbeingStore extends SyncedStore<WellbeingDoc> {
  private reporter = inject(ConflictReporter);

  /** Newest first. */
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
      identifier: 'wellbeing-http-sync-v2',
      path: '/api/sync/wellbeing',
      label: 'wellbeing sync',
      trashKind: 'wellbeing',
      migrationStrategies,
    };
  }

  /** Returns the new check-in's ulid. */
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
