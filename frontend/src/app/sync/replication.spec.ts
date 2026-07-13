import { signal } from '@angular/core';
import { describe, expect, it, vi } from 'vitest';

import { AuthState } from './auth-state';
import { guardAuth } from './replication';

/** Minimal Response stand-in — guardAuth only reads status/redirected/headers. */
function res(over: { status?: number; contentType?: string | null; redirected?: boolean }): Response {
  return {
    status: over.status ?? 200,
    redirected: over.redirected ?? false,
    headers: new Headers(over.contentType === null ? {} : { 'content-type': over.contentType ?? 'application/json' }),
  } as Response;
}

describe('guardAuth — expired-session detection on sync fetches', () => {
  it('lets a healthy JSON response through and leaves the error signal alone', () => {
    const err = signal<string | null>(null);
    const lost = vi.fn();
    expect(() => guardAuth(res({}), err, lost)).not.toThrow();
    expect(err()).toBeNull();
    expect(lost).not.toHaveBeenCalled();
  });

  it('flags 401 and 403 as login-required', () => {
    for (const status of [401, 403]) {
      const err = signal<string | null>(null);
      expect(() => guardAuth(res({ status }), err)).toThrow('auth-required');
      expect(err()).toContain('login required');
    }
  });

  it('tells the caller the session is gone, so replication can stand down', () => {
    // The bug this pins (2026-07-13): a 401 was treated as transient, so a
    // signed-out tab re-ran the sync every 5s forever — never recovering, never
    // saying anything, and taking a 401 each time. Only a fresh login can help,
    // so the guard must report the loss rather than just throw.
    for (const status of [401, 403]) {
      const lost = vi.fn();
      expect(() => guardAuth(res({ status }), signal<string | null>(null), lost)).toThrow();
      expect(lost).toHaveBeenCalledOnce();
    }
    // A redirect to the login page is the same story, wearing a 200.
    const lost = vi.fn();
    expect(() =>
      guardAuth(res({ redirected: true, contentType: 'text/html' }), signal<string | null>(null), lost),
    ).toThrow();
    expect(lost).toHaveBeenCalledOnce();
  });

  it('does NOT report the session lost for a plain server error', () => {
    // A 500 IS transient — retrying is right, and flipping the app to signed-out
    // would throw the user onto a login page over a blip.
    const lost = vi.fn();
    expect(() => guardAuth(res({ status: 500 }), signal<string | null>(null), lost)).not.toThrow();
    expect(lost).not.toHaveBeenCalled();
  });

  it('flags a followed redirect — the stale-cookie 302→login-page→200 case', () => {
    const err = signal<string | null>(null);
    expect(() => guardAuth(res({ redirected: true, contentType: 'text/html' }), err)).toThrow('auth-required');
    expect(err()).toContain('login required');
  });

  it('flags a non-JSON body even on 200 — HTML where JSON was expected', () => {
    const err = signal<string | null>(null);
    expect(() => guardAuth(res({ contentType: 'text/html' }), err)).toThrow('auth-required');
  });

  it('flags a missing content-type', () => {
    const err = signal<string | null>(null);
    expect(() => guardAuth(res({ contentType: null }), err)).toThrow('auth-required');
  });

  it('does NOT flag a JSON error status like 500 — that is a plain retry, not auth', () => {
    const err = signal<string | null>(null);
    expect(() => guardAuth(res({ status: 500 }), err)).not.toThrow();
    expect(err()).toBeNull();
  });
});

describe('AuthState', () => {
  it('starts hopeful and latches once the session is gone', () => {
    const auth = new AuthState();
    expect(auth.lost()).toBe(false);
    auth.lose();
    expect(auth.lost()).toBe(true);
    // Idempotent: every collection's replication reports the same 401, and the
    // shell must not thrash between states because three stores each said so.
    auth.lose();
    expect(auth.lost()).toBe(true);
  });
});
