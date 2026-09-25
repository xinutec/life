import { Injectable, signal } from '@angular/core';

/** App-wide "the server says we are signed out" flag.
 *
 *  Replication finds out first: it polls, so an expired session surfaces there
 *  before the user touches anything. The stores raise this, the shell drops to
 *  the sign-in prompt, and replication stops rather than retrying a 401
 *  forever. NOT the same as offline: offline means "try again later, your
 *  writes are safe"; this means "only a fresh login gets you further". */
@Injectable({ providedIn: 'root' })
export class AuthState {
  /** True once a sync fetch has been refused for want of a session. */
  readonly lost = signal(false);

  /** The session is gone — halt replication and show the sign-in prompt. */
  lose(): void {
    this.lost.set(true);
  }
}
