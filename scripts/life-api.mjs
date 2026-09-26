#!/usr/bin/env node
// One authenticated request against a running Life, made by the signed-in
// ChromeDebug tab (production has no `dev-login`, and we never handle
// credentials). Needs a Life tab open (xinutec-infra/mac-mini/chrome-debug.sh
// start).
//
//   ./scripts/life-api.mjs GET  /api/items
//   ./scripts/life-api.mjs PATCH /api/items/16 '{"name":"...","category":"food"}'
//
// Chain steps with jq rather than writing per-task seed scripts:
//
//   room=$(./scripts/life-api.mjs POST /api/locations '{"kind":"room","name":"X"}' | jq .id)
//   ./scripts/life-api.mjs POST /api/items "{\"name\":\"Y\",\"location_id\":$room}"
import { execFileSync } from 'node:child_process';

const CDP = process.env.CDP_PY
  ?? `${process.env.HOME}/Code/xinutec-infra/mac-mini/browser/cdp.py`;
const BASE = process.env.LIFE_BASE ?? 'https://life.xinutec.org';

const [method, path, body] = process.argv.slice(2);
if (!method || !path) {
  console.error('usage: life-api.mjs <GET|POST|PATCH|DELETE> <path> [json-body]');
  console.error('   e.g. life-api.mjs POST /api/locations \'{"kind":"room","name":"Bedroom"}\'');
  process.exit(2);
}
if (!path.startsWith('/')) throw new Error(`path must start with "/": ${path}`);
if (body !== undefined) JSON.parse(body); // fail here, not inside the browser

// `eval` against an already-open tab, NOT `run`: `run` navigates, which tears
// down the SPA's context mid-script — after its first write has landed.
const host = new URL(BASE).host;
const js = `
(async () => {
  const res = await fetch(${JSON.stringify(BASE + path)}, {
    method: ${JSON.stringify(method.toUpperCase())},
    credentials: 'include',
    ${body === undefined ? '' : `headers: { 'Content-Type': 'application/json' },
    body: ${JSON.stringify(body)},`}
  });
  const text = await res.text();
  // The status rides along: a 4xx with an empty body is otherwise
  // indistinguishable from a successful request that returned nothing, and
  // "it did nothing" is the wrong thing to conclude from a refusal.
  return JSON.stringify({ status: res.status, ok: res.ok, body: text });
})()
`;

const raw = execFileSync(CDP, ['eval', js, host], {
  encoding: 'utf8',
  maxBuffer: 32 * 1024 * 1024,
});

// cdp.py prints the JS value as a JSON string; unwrap it once, then again for
// our own envelope.
const { status, ok, body: text } = JSON.parse(JSON.parse(raw));
if (!ok) {
  console.error(`life-api: ${method} ${path} -> HTTP ${status}`);
  if (text) console.error(text.slice(0, 2000));
  process.exit(1);
}
// Pretty-print JSON, pass anything else through untouched.
try {
  console.log(JSON.stringify(JSON.parse(text), null, 2));
} catch {
  console.log(text);
}
