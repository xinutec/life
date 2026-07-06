import { HttpErrorResponse } from '@angular/common/http';
import { describe, expect, it } from 'vitest';

import { classifyApiError } from './api-error';

/** HttpErrorResponse stand-in — classifyApiError only reads `.status`. */
function httpError(status: number): HttpErrorResponse {
  return new HttpErrorResponse({ status });
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
