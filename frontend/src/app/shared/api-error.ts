import { HttpErrorResponse } from '@angular/common/http';

/** How an API call failed. Offline must never read as signed out, which would
 *  show offline users the sign-in screen. */
export type ApiFailure =
  | { readonly kind: 'offline' }
  | { readonly kind: 'unauthenticated' }
  | { readonly kind: 'server'; readonly status: number };

/** The one place that reads an HttpErrorResponse's status (a dev-lint rule).
 *  Status 0 is a dropped connection. */
export function classifyApiError(e: unknown): ApiFailure {
  if (e instanceof HttpErrorResponse) {
    if (e.status === 0) return { kind: 'offline' };
    if (e.status === 401 || e.status === 403) return { kind: 'unauthenticated' };
    return { kind: 'server', status: e.status };
  }
  // Never sign anyone out on a stray throw.
  return { kind: 'offline' };
}

/** The same for a raw `fetch()`. Signed out takes a positive signal: 401/403, a
 *  redirect, or a 2xx login page; the service worker's offline 504 is not one. */
export function classifyFetchResponse(res: Response): { kind: 'ok' } | ApiFailure {
  if (res.status === 401 || res.status === 403) return { kind: 'unauthenticated' };
  const json = (res.headers.get('content-type') ?? '').includes('application/json');
  if (res.redirected || (res.ok && !json)) return { kind: 'unauthenticated' };
  if (res.status === 504) return { kind: 'offline' }; // ngsw's synthetic offline answer
  if (!res.ok) return { kind: 'server', status: res.status };
  return { kind: 'ok' };
}

/** " — are you online?" for a dropped connection, else ''. */
export function onlineHint(e: unknown): string {
  return classifyApiError(e).kind === 'offline' ? ' — are you online?' : '';
}

export function isNotFound(e: unknown): boolean {
  const f = classifyApiError(e);
  return f.kind === 'server' && f.status === 404;
}

/** `default: assertNever(f)` makes a new failure kind a compile error. */
export function assertNever(x: never): never {
  throw new Error(`unhandled ApiFailure: ${JSON.stringify(x)}`);
}
