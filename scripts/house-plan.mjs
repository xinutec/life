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
// Seen from above, a wall unit and the base unit under it occupy the same
// square and their numbers land on top of each other. Height is the thing that
// tells them apart, so --iso keeps it.
const iso = argv.includes('--iso');

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

if (iso) {
  // A 2:1 isometric: x goes right-and-down, z goes left-and-down, y straight up.
  // Not a perspective camera — parallel projection keeps a box the same size
  // wherever it sits, so two cupboards of equal width read as equal, which is
  // what makes them identifiable.
  const S = 78; // px per metre
  const px = (x, y, z) => [(x - z) * 0.866 * S, ((x + z) * 0.5 - y) * S];

  const corners = [];
  for (const box of boxes) {
    const { cx, cz, w, d, h } = box;
    const y0 = box.y0 ?? 0;
    for (const X0 of [cx - w / 2, cx + w / 2])
      for (const Z0 of [cz - d / 2, cz + d / 2])
        for (const Y0 of [y0, y0 + h]) corners.push(px(X0, Y0, Z0));
  }
  const xs = corners.map((c) => c[0]);
  const ys = corners.map((c) => c[1]);
  const m = 40;
  const minX = Math.min(...xs) - m;
  const minY = Math.min(...ys) - m;
  const wSvg = Math.max(...xs) - minX + m;
  const hSvg = Math.max(...ys) - minY + m;
  const P = (x, y, z) => {
    const [a, b2] = px(x, y, z);
    return `${(a - minX).toFixed(1)},${(b2 - minY).toFixed(1)}`;
  };

  // Painter's algorithm: draw what is furthest away first. Depth here is
  // x + z (how far back along both floor axes) with height as a tie-break, so a
  // wall unit is drawn after the counter it hangs over rather than behind it.
  const order = [...boxes].sort(
    (p1, p2) => p1.cx + p1.cz - (p2.cx + p2.cz) || (p1.y0 ?? 0) - (p2.y0 ?? 0),
  );

  const out = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${wSvg.toFixed(0)}" height="${hSvg.toFixed(0)}" viewBox="0 0 ${wSvg.toFixed(0)} ${hSvg.toFixed(0)}">`,
    `<rect width="100%" height="100%" fill="#fbfbf7"/>`,
  ];
  const labels = [];
  for (const box of order) {
    const { cx, cz, w, d, h } = box;
    const y0 = box.y0 ?? 0;
    const x0 = cx - w / 2;
    const x1 = cx + w / 2;
    const z0 = cz - d / 2;
    const z1 = cz + d / 2;
    const y1 = y0 + h;
    const fill = box.color ?? '#cfd8c8';
    // Three visible faces, shaded so the form reads: top lightest, then the two
    // sides. A single flat colour makes a row of boxes one indistinct slab.
    out.push(
      `<polygon points="${P(x0, y1, z0)} ${P(x1, y1, z0)} ${P(x1, y1, z1)} ${P(x0, y1, z1)}" fill="${fill}" stroke="#4a4a44" stroke-width="1"/>`,
      `<polygon points="${P(x0, y0, z1)} ${P(x1, y0, z1)} ${P(x1, y1, z1)} ${P(x0, y1, z1)}" fill="${fill}" fill-opacity="0.78" stroke="#4a4a44" stroke-width="1"/>`,
      `<polygon points="${P(x1, y0, z0)} ${P(x1, y0, z1)} ${P(x1, y1, z1)} ${P(x1, y1, z0)}" fill="${fill}" fill-opacity="0.58" stroke="#4a4a44" stroke-width="1"/>`,
    );
    const [lx, ly] = P(cx, y1, cz).split(',');
    labels.push({ n: box.n, x: Number(lx), y: Number(ly) + 5 });
  }
  // ⚠ EVERY label after ALL the geometry. Drawn with its box, a number is
  // painted over by whatever is drawn in front of it — 25 and 20 came out as
  // ghosts under the units in front, which is precisely the boxes a person most
  // needs to name.
  //
  // Nudged apart where two land within a few pixels: coincident labels are as
  // unreadable as hidden ones, and this is a picture whose whole job is to let
  // somebody say a number.
  labels.sort((a, b2) => a.y - b2.y || a.x - b2.x);
  for (const [i, l] of labels.entries()) {
    for (const other of labels.slice(0, i)) {
      if (Math.abs(other.x - l.x) < 16 && Math.abs(other.y - l.y) < 14) l.y = other.y + 15;
    }
  }
  for (const l of labels) {
    out.push(
      `<text x="${l.x.toFixed(1)}" y="${l.y.toFixed(1)}" text-anchor="middle" font-family="system-ui" font-size="15" font-weight="700" fill="#111" stroke="#fbfbf7" stroke-width="3.5" paint-order="stroke">${l.n}</text>`,
    );
  }
  out.push('</svg>');
  console.log(out.join('\n'));
} else {
  console.log(parts.join('\n'));
}
console.error(`${boxes.length} boxes drawn (numbered by index in scenes/house.json)`);
