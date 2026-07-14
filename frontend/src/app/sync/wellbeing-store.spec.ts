import { describe, expect, it } from 'vitest';

import { migrationStrategies } from './wellbeing-store';

/** Pure-function tests for the RxDB schema migrations. A device that hasn't
 *  opened the app since before a schema bump runs these once on next open, so
 *  the polarity flip (fatigue → energy) in particular must be exact. */
describe('wellbeing migrationStrategies', () => {
  it('v1 adds fatigue = null to a pre-fatigue doc', () => {
    const out = migrationStrategies[1]({ ulid: 'u', score: 3 });
    expect(out).toMatchObject({ ulid: 'u', score: 3, fatigue: null });
  });

  it('v1 leaves an existing fatigue untouched', () => {
    expect(migrationStrategies[1]({ fatigue: 4 })).toMatchObject({ fatigue: 4 });
  });

  it('v2 adds emotions = [] to a pre-emotions doc', () => {
    const out = migrationStrategies[2]({ ulid: 'u' });
    expect(out).toMatchObject({ ulid: 'u', emotions: [] });
  });

  it('v2 leaves existing emotions untouched', () => {
    expect(migrationStrategies[2]({ emotions: ['Anxious'] })).toMatchObject({
      emotions: ['Anxious'],
    });
  });

  it('v3 flips fatigue → energy as the 6 − fatigue complement', () => {
    // none (fatigue 1, best) → energetic (energy 5); severe (fatigue 5, worst)
    // → drained (energy 1). Higher = better on both axes afterwards.
    for (const [fatigue, energy] of [
      [1, 5],
      [2, 4],
      [3, 3],
      [4, 2],
      [5, 1],
    ]) {
      expect(migrationStrategies[3]({ fatigue })).toMatchObject({ energy });
    }
  });

  it('v3 maps a null (unrecorded) fatigue to null energy', () => {
    expect(migrationStrategies[3]({ fatigue: null })).toMatchObject({ energy: null });
  });

  it('v3 maps a missing fatigue field to null energy', () => {
    expect(migrationStrategies[3]({ ulid: 'u' })).toMatchObject({ ulid: 'u', energy: null });
  });

  it('v3 drops the old fatigue key (renamed, not duplicated)', () => {
    expect(migrationStrategies[3]({ fatigue: 2 })).not.toHaveProperty('fatigue');
  });

  it('v4 rescales the readings to tenths', () => {
    const out = migrationStrategies[4]({ ulid: 'u', score: 4, energy: 2 });
    expect(out).toMatchObject({ ulid: 'u', scoreTenths: 40, energyTenths: 20 });
  });

  it('v4 keeps an unrecorded energy unrecorded (not a zero)', () => {
    // 0 tenths would be a reading below the bottom of the scale — and would say
    // he felt drained on every check-in where he never said anything at all.
    expect(migrationStrategies[4]({ score: 3, energy: null })).toMatchObject({
      energyTenths: null,
    });
  });

  it('v4 drops the old point-scale keys, so nothing can read a 4 as a 0.4', () => {
    const out = migrationStrategies[4]({ score: 4, energy: 4 });
    expect(out).not.toHaveProperty('score');
    expect(out).not.toHaveProperty('energy');
  });

  it('the whole chain carries an original check-in through to tenths', () => {
    // The device that has been closed since v0: every strategy runs in turn. A 4
    // logged a year ago must still be a 4 — this chain is the only thing standing
    // between his history and a silently rescaled one.
    let doc: Record<string, unknown> = { ulid: 'u', score: 4, note: 'gym day' };
    for (const v of [1, 2, 3, 4] as const) doc = migrationStrategies[v](doc);
    expect(doc).toMatchObject({
      ulid: 'u',
      scoreTenths: 40, // still a 4 out of 5
      energyTenths: null, // never recorded one back then
      emotions: [],
      note: 'gym day',
    });
  });
});
