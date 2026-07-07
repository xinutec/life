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
});
