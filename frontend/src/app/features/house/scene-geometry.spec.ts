import { describe, expect, it } from 'vitest';

import { Room } from '../../models';
import { bounds, perimeter, roomPerimeter, segments } from './scene-geometry';

const close = (a: number, b: number) => expect(a).toBeCloseTo(b, 6);

describe('perimeter', () => {
  it('walks a square: four 90° turns come back to the start', () => {
    // First wall heads +x (turn 0), then three right turns.
    const pts = perimeter([
      [0, 4],
      [90, 4],
      [90, 4],
      [90, 4],
    ]);
    expect(pts).toHaveLength(5);
    close(pts[1].x, 4);
    close(pts[1].z, 0);
    close(pts[2].x, 4);
    close(pts[2].z, 4);
    close(pts[3].x, 0);
    close(pts[3].z, 4);
    close(pts[4].x, 0); // closed loop
    close(pts[4].z, 0);
  });

  it('honours the start corner and initial heading', () => {
    const pts = perimeter([[0, 2]], { x: 10, z: 5 }, 90); // 90° = +z
    close(pts[0].x, 10);
    close(pts[0].z, 5);
    close(pts[1].x, 10);
    close(pts[1].z, 7);
  });
});

describe('roomPerimeter', () => {
  it('reads start/heading/walls off the Room shape (heading defaults to 0)', () => {
    const room: Room = { start: [1, 2], walls: [[0, 3]] };
    const pts = roomPerimeter(room);
    close(pts[1].x, 4);
    close(pts[1].z, 2);
  });
});

describe('segments', () => {
  it('turns n points into n-1 consecutive wall segments', () => {
    const segs = segments([
      { x: 0, z: 0 },
      { x: 4, z: 0 },
      { x: 4, z: 4 },
    ]);
    expect(segs).toEqual([
      { ax: 0, az: 0, bx: 4, bz: 0 },
      { ax: 4, az: 0, bx: 4, bz: 4 },
    ]);
  });

  it('is empty for fewer than two points', () => {
    expect(segments([])).toEqual([]);
    expect(segments([{ x: 1, z: 1 }])).toEqual([]);
  });
});

describe('bounds', () => {
  it('is null with no points (nothing to frame)', () => {
    expect(bounds([])).toBeNull();
  });

  it('centres on the extent and spans the larger axis', () => {
    const b = bounds([
      { x: 0, z: 0 },
      { x: 4, z: 2 },
    ])!;
    close(b.cx, 2);
    close(b.cz, 1);
    close(b.span, 4);
  });

  it('never reports a span below 1 (a single point still frames)', () => {
    const b = bounds([{ x: 3, z: 3 }])!;
    close(b.cx, 3);
    close(b.cz, 3);
    close(b.span, 1);
  });
});
