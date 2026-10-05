import { Injectable, signal } from '@angular/core';

/** The server says we are signed out. Raised by replication, which polls and
 *  so finds out first. Not offline: only a fresh login gets further. */
@Injectable({ providedIn: 'root' })
export class AuthState {
  readonly lost = signal(false);

  lose(): void {
    this.lost.set(true);
  }
}
