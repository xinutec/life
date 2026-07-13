import { Injectable, signal } from '@angular/core';

/** App-wide "the server says we are signed out" flag.
 *
 *  Replication is the first thing to find out: it polls on a timer, so an
 *  expired session surfaces there long before the user touches anything. Without
 *  somewhere to report it, a signed-out tab retried the sync every 5s forever —
 *  taking a 401 each time, telling the user nothing, and hammering the server for
 *  as long as the tab stayed open (observed 2026-07-13: a background tab had been
 *  doing exactly that since its session lapsed that morning).
 *
 *  So the stores raise this, the shell watches it and drops to the sign-in
 *  prompt, and replication stops. Deliberately NOT the same thing as being
 *  offline: offline means "try again later, your writes are safe"; this means
 *  "only a fresh login gets you further". */
@Injectable({ providedIn: 'root' })
export class AuthState {
  /** True once a sync fetch has been refused for want of a session. */
  readonly lost = signal(false);

  /** The session is gone — halt replication and show the sign-in prompt. */
  lose(): void {
    this.lost.set(true);
  }
}
