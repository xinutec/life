import { HttpErrorResponse } from '@angular/common/http';
import { describe, expect, it } from 'vitest';

import { classifyApiError, classifyFetchResponse } from './api-error';

/** HttpErrorResponse stand-in — classifyApiError only reads `.status`. */
function httpError(status: number): HttpErrorResponse {
  return new HttpErrorResponse({ status });
}

/** Response stand-in — classifyFetchResponse reads status/ok/redirected/headers. */
function fetchRes(over: {
  status?: number;
  contentType?: string | null;
  redirected?: boolean;
}): Response {
  const status = over.status ?? 200;
  return {
    status,
    ok: status >= 200 && status < 300,
    redirected: over.redirected ?? false,
    headers: new Headers(
      over.contentType === null ? {} : { 'content-type': over.contentType ?? 'application/json' },
    ),
  } as Response;
}

describe('classifyApiError — the HttpClient error boundary', () => {
  it('maps status 0 (dropped connection under withFetch) to offline', () => {
    expect(classifyApiError(httpError(0))).toEqual({ kind: 'offline' });
  });

  it('maps 401 and 403 to unauthenticated — the only case that means logged out', () => {
    expect(classifyApiError(httpError(401))).toEqual({ kind: 'unauthenticated' });
    expect(classifyApiError(httpError(403))).toEqual({ kind: 'unauthenticated' });
  });

  it('keeps other statuses as server errors with the code intact (404, 500)', () => {
    expect(classifyApiError(httpError(404))).toEqual({ kind: 'server', status: 404 });
    expect(classifyApiError(httpError(500))).toEqual({ kind: 'server', status: 500 });
  });

  it('does not treat a server 500 or 404 as offline or unauthenticated', () => {
    // Regression guard: the offline bug came from collapsing every error into one
    // bucket. A 5xx must stay distinct from a dropped connection and from auth.
    expect(classifyApiError(httpError(500)).kind).toBe('server');
    expect(classifyApiError(httpError(404)).kind).toBe('server');
  });

  it('treats a non-HTTP throw as offline rather than a confirmed auth failure', () => {
    expect(classifyApiError(new Error('boom'))).toEqual({ kind: 'offline' });
    expect(classifyApiError(undefined)).toEqual({ kind: 'offline' });
  });
});

describe('classifyFetchResponse — the raw-fetch sibling, same taxonomy', () => {
  it('passes a healthy JSON response through as ok', () => {
    expect(classifyFetchResponse(fetchRes({}))).toEqual({ kind: 'ok' });
    // An error status wearing JSON is a server fault the caller may retry —
    // never silently "ok", never auth.
    expect(classifyFetchResponse(fetchRes({ status: 500 }))).toEqual({ kind: 'server', status: 500 });
  });

  it('maps only positive auth signals to unauthenticated', () => {
    expect(classifyFetchResponse(fetchRes({ status: 401 })).kind).toBe('unauthenticated');
    expect(classifyFetchResponse(fetchRes({ status: 403 })).kind).toBe('unauthenticated');
    // Stale cookie: 302 followed to a login page, arriving as a 200 HTML body.
    expect(classifyFetchResponse(fetchRes({ redirected: true, contentType: 'text/html' })).kind).toBe(
      'unauthenticated',
    );
    // The login page served directly on a 200 where JSON was expected.
    expect(classifyFetchResponse(fetchRes({ contentType: 'text/html' })).kind).toBe('unauthenticated');
    expect(classifyFetchResponse(fetchRes({ contentType: null })).kind).toBe('unauthenticated');
  });

  it('maps the service worker’s bodiless synthetic 504 to offline, never auth', () => {
    // ngsw intercepts every fetch and answers network failure with a 504 that
    // has no body and no content-type. Reading that as "logged out" is the
    // 2026-07-16 offline-boot bug; it is the offline signal.
    expect(classifyFetchResponse(fetchRes({ status: 504, contentType: null }))).toEqual({
      kind: 'offline',
    });
  });

  it('maps a non-ok non-JSON response (ingress HTML error page) to server, never auth', () => {
    expect(classifyFetchResponse(fetchRes({ status: 502, contentType: 'text/html' }))).toEqual({
      kind: 'server',
      status: 502,
    });
    expect(classifyFetchResponse(fetchRes({ status: 503, contentType: 'text/html' }))).toEqual({
      kind: 'server',
      status: 503,
    });
  });
});
