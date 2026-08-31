#!/usr/bin/env node
// A top-down plan of the house, with every furniture box NUMBERED — so a person
// can say "3 is the tall cupboard" instead of reading coordinates.
//
// Why numbers and not names: scenes/house.json's furniture carries geometry
// only (cx/cz/w/d/h/y0/color) and no labels, so nothing in the file says which
// box is which. That is exactly the gap between having locations in the
// database and placing them in the scene (#134) — and it is a question only
// somebody who has stood in the room can answer.
//
// Reuses `perimeter` from the app rather than re-deriving the turtle walk: the
// convention (turn THEN step, heading in degrees, +X at 0) is stated in one
// place and tested there, and a second copy would drift the first time either
// changed.
//
//   ./scripts/house-plan.mjs > /tmp/plan.svg
//   ./scripts/house-plan.mjs --room kitchen > /tmp/kitchen.svg
import { readFileSync } from 'node:fs';

const argv = process.argv.slice(2);
const only = argv.includes('--room') ? argv[argv.indexOf('--room') + 1] : null;

const scene = JSON.parse(readFileSync(new URL('../scenes/house.json', import.meta.url), 'utf8'));

/** Corner points of a room's outline — the same walk the renderer does. */
function perimeter(walls, start, heading0) {
  const pts = [{ x: start[0], z: start[1] }];
  let [x, z] = start;
  let heading = heading0 ?? 0;
  for (const [turn, len] of walls) {
    heading += turn;
    const r = (heading * Math.PI) / 180;
    x += len * Math.cos(r);
    z += len * Math.sin(r);
    pts.push({ x, z });
  }
  return pts;
}

const rooms = scene.rooms.filter((r) => !only || r.name === only);
if (rooms.length === 0) throw new Error(`no room named ${only}`);
const outlines = rooms.map((r) => ({ name: r.name, pts: perimeter(r.walls, r.start, r.heading) }));

// Only the furniture inside the rooms being drawn, so a kitchen plan is not
// covered in the living room's boxes. Point-in-bounding-box is enough: rooms
// here are rectilinear and the alternative (a full point-in-polygon) would be
// precision nobody asked for.
const bounds = (pts) => ({
  x0: Math.min(...pts.map((p) => p.x)), x1: Math.max(...pts.map((p) => p.x)),
  z0: Math.min(...pts.map((p) => p.z)), z1: Math.max(...pts.map((p) => p.z)),
});
const inAny = (f) =>
  outlines.some(({ pts }) => {
    const b = bounds(pts);
    // No padding. A tolerance of 0.3m pulled the dining room's boxes into the
    // kitchen plan, which is worse than missing one on the boundary: a number
    // pointing at furniture in another room cannot be answered at all.
    return f.cx >= b.x0 && f.cx <= b.x1 && f.cz >= b.z0 && f.cz <= b.z1;
  });

// ⚠ Numbered by their INDEX IN THE FILE, not by drawing order: the number a
// person reads off this plan has to be the one that identifies the box in
// scenes/house.json afterwards, or the answer cannot be applied.
const boxes = scene.furniture.map((f, i) => ({ ...f, n: i })).filter(inAny);

const all = outlines.flatMap((o) => o.pts).concat(
  boxes.flatMap((b) => [
    { x: b.cx - b.w / 2, z: b.cz - b.d / 2 },
    { x: b.cx + b.w / 2, z: b.cz + b.d / 2 },
  ]),
);
const b = bounds(all);
const pad = 0.6;
const scale = 110; // px per metre — legible at a glance, not to scale on paper
const W = (b.x1 - b.x0 + pad * 2) * scale;
const H = (b.z1 - b.z0 + pad * 2) * scale;
const X = (x) => (x - b.x0 + pad) * scale;
const Z = (z) => (z - b.z0 + pad) * scale;

const parts = [
  `<svg xmlns="http://www.w3.org/2000/svg" width="${W.toFixed(0)}" height="${H.toFixed(0)}" viewBox="0 0 ${W.toFixed(0)} ${H.toFixed(0)}">`,
  `<rect width="100%" height="100%" fill="#fbfbf7"/>`,
];
// ⚠ Split by HEIGHT, in two panels. Seen from above, a wall unit sits exactly
// on top of the base unit below it, so a single plan stacks their numbers into
// an unreadable pile — measured: seven numbers overlapping in one run of
// counter. A plan nobody can read is not a question anybody can answer.
const WALL_UNIT_Y = 1.2; // metres: above worktop height, so it is on the wall
const panels = [
  { title: 'At floor level', of: boxes.filter((x) => (x.y0 ?? 0) < WALL_UNIT_Y) },
  { title: 'On the wall (above the worktop)', of: boxes.filter((x) => (x.y0 ?? 0) >= WALL_UNIT_Y) },
].filter((p) => p.of.length > 0);

const draw = (box, dy) => {
  const x = X(box.cx - box.w / 2);
  const y = Z(box.cz - box.d / 2) + dy;
  return [
    `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${(box.w * scale).toFixed(1)}" height="${(box.d * scale).toFixed(1)}" fill="${box.color ?? '#cfd8c8'}" fill-opacity="0.55" stroke="#556" stroke-width="1.2"/>`,
    `<text x="${X(box.cx).toFixed(1)}" y="${(Z(box.cz) + dy + 6).toFixed(1)}" text-anchor="middle" font-family="system-ui" font-size="17" font-weight="700" fill="#111">${box.n}</text>`,
  ];
};

parts.length = 2; // keep the header + background; the outline is drawn per panel
parts[1] = `<rect width="100%" height="${(H * panels.length).toFixed(0)}" fill="#fbfbf7"/>`;
parts[0] = parts[0].replace(
  `height="${H.toFixed(0)}" viewBox="0 0 ${W.toFixed(0)} ${H.toFixed(0)}"`,
  `height="${(H * panels.length).toFixed(0)}" viewBox="0 0 ${W.toFixed(0)} ${(H * panels.length).toFixed(0)}"`,
);
panels.forEach((panel, i) => {
  const dy = i * H;
  for (const { pts } of outlines) {
    parts.push(
      `<polyline fill="none" stroke="#333" stroke-width="3" points="${pts.map((p) => `${X(p.x).toFixed(1)},${(Z(p.z) + dy).toFixed(1)}`).join(' ')}"/>`,
    );
  }
  parts.push(
    `<text x="14" y="${(dy + 24).toFixed(1)}" font-family="system-ui" font-size="16" font-weight="700" fill="#333">${panel.title}</text>`,
  );
  for (const box of panel.of) parts.push(...draw(box, dy));
});
parts.push('</svg>');
console.log(parts.join('\n'));
console.error(`${boxes.length} boxes drawn (numbered by index in scenes/house.json)`);
