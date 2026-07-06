import { HttpErrorResponse } from '@angular/common/http';

/** The three ways an HttpClient call can fail that the app makes decisions on.
 *  A discriminated union so callers `switch (f.kind)` exhaustively — the compiler
 *  then forces every site to handle `offline` distinctly from `unauthenticated`,
 *  which is the whole point: a network failure must never be mistaken for a
 *  logged-out session (that mistake showed the sign-in screen to offline users). */
export type ApiFailure =
  | { readonly kind: 'offline' }
  | { readonly kind: 'unauthenticated' }
  | { readonly kind: 'server'; readonly status: number };

/** Classify an HttpClient error at the boundary. This is the ONE place allowed to
 *  read the raw status off an `HttpErrorResponse`; every callsite consumes the
 *  `ApiFailure` instead (enforced by dev-lint's DL-ANGULAR-HTTP-ERROR-CLASSIFIED).
 *  It is the HttpClient-world sibling of sync/replication.ts's `guardAuth`, which
 *  does the same job for the fetch-based RxDB replication. Total over `unknown`
 *  so it can sit in any `error:` handler. `withFetch()` reports a dropped
 *  connection as status 0 — that is the offline signal we must not confuse with
 *  an auth failure. */
export function classifyApiError(e: unknown): ApiFailure {
  if (e instanceof HttpErrorResponse) {
    if (e.status === 0) return { kind: 'offline' };
    if (e.status === 401 || e.status === 403) return { kind: 'unauthenticated' };
    return { kind: 'server', status: e.status };
  }
  // HttpClient always emits HttpErrorResponse on its error channel; a non-HTTP
  // throw reaching here is unexpected, so treat it as an unreachable-server case
  // rather than a confirmed auth failure — never force a sign-out on a stray throw.
  return { kind: 'offline' };
}

/** The standard "are you online?" suffix for a failed write — empty unless the
 *  failure was a dropped connection. Keeps the many `Could not ${what}` toasts
 *  from each re-deriving offline-ness off a raw status. */
export function onlineHint(e: unknown): string {
  return classifyApiError(e).kind === 'offline' ? ' — are you online?' : '';
}

/** True when the failure was a 404 — the "this specific thing isn't there" case,
 *  distinct from offline / auth / other server errors. Lets a lookup toast say
 *  "not found" vs "are you online?" without re-reading the raw status. */
export function isNotFound(e: unknown): boolean {
  const f = classifyApiError(e);
  return f.kind === 'server' && f.status === 404;
}

/** Exhaustiveness guard: `default: assertNever(f)` in a `switch (f.kind)` makes
 *  the compiler reject any future ApiFailure kind a callsite forgets to handle. */
export function assertNever(x: never): never {
  throw new Error(`unhandled ApiFailure: ${JSON.stringify(x)}`);
}
