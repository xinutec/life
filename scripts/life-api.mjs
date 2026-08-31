#!/usr/bin/env node
// One authenticated request against a RUNNING Life, borrowing the browser's
// session instead of handling anybody's credentials.
//
// Why this exists rather than another curl script: seed-demo.sh talks to a local
// server and logs in through `dev-login`, which production does not have. The
// live app authenticates through Nextcloud, and its session belongs to a person
// who logged in by hand — so the only honest way to reach it is to ask the
// browser that already holds that session to make the call.
//
//   ./scripts/life-api.mjs GET  /api/items
//   ./scripts/life-api.mjs POST /api/locations '{"kind":"room","name":"Bedroom"}'
//   ./scripts/life-api.mjs PATCH /api/items/16 '{"name":"...","category":"food"}'
//
// Composes with jq, which is how multi-step work is done — no bespoke seeding
// script per task, and no personal data in this public repo:
//
//   room=$(./scripts/life-api.mjs POST /api/locations '{"kind":"room","name":"X"}' | jq .id)
//   ./scripts/life-api.mjs POST /api/items "{\"name\":\"Y\",\"location_id\":$room}"
//
// Requires ChromeDebug running with a Life tab open and signed in
// (xinutec-infra/mac-mini/chrome-debug.sh start).
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

// `eval` against an already-open tab, NOT `run`. `run` navigates, and this app
// is a single-page app: the navigation tears down the execution context
// mid-script, so a multi-step call dies with "Inspected target navigated or
// closed" AFTER its first write has already landed. Measured the hard way.
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
