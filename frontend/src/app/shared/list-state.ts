import { Component, effect, input, output, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';

/** The loading / error / empty triad every list screen shows before its data.
 *  One place for the spinner, the retry-on-error line and the empty-state line,
 *  so the list screens stop each re-inventing (and mis-styling) them.
 *
 *  It renders ONLY the status line — the list itself stays in the host template
 *  and is naturally empty while data is loading or absent. Place it above the
 *  list:
 *
 *    <app-list-state [loading]="!loaded()" [empty]="items().length === 0"
 *                    emptyText="No items yet." emptyIcon="inventory_2" />
 *    <mat-list> … </mat-list>
 *
 *  HTTP-backed screens that can fail to load pass [error] and handle (retry);
 *  RxDB-backed screens (which can't fail to load) omit both. */
@Component({
  selector: 'app-list-state',
  templateUrl: './list-state.html',
  styleUrl: './list-state.scss',
  imports: [MatButtonModule, MatIconModule, MatProgressBarModule],
})
export class ListState {
  /** Data hasn't produced its first result yet → spinner. */
  readonly loading = input(false);
  /** The load failed (not the same as "empty") → message + Retry. */
  readonly error = input(false);
  /** Loaded successfully but there's nothing to show → empty message. */
  readonly empty = input(false);
  /** A background refresh over already-shown content (a revisit re-fetch). Shows
   *  a thin progress bar OVER the content — but only if the refresh outlives a
   *  short delay, so a fast refresh stays silent and the bar never flashes. This
   *  is the feature-tab counterpart of the app shell's refresh cue. RxDB screens
   *  don't pass it (their data can't be "refreshing" this way). */
  readonly refreshing = input(false);

  readonly emptyText = input('Nothing here yet.');
  /** Optional Material icon name shown above the empty message. */
  readonly emptyIcon = input<string | null>(null);
  readonly errorText = input('Couldn’t load — are you online?');

  /** Emitted when the user taps Retry in the error state. */
  readonly retry = output<void>();

  /** True once a refresh has been in flight past the delay — drives the bar. */
  private readonly _showRefresh = signal(false);
  protected readonly showRefresh = this._showRefresh.asReadonly();
  /** Below this, a refresh is "instant" and shows nothing; above it, the bar
   *  appears. Matches the app shell's 400ms gate. */
  private static readonly REVEAL_MS = 400;

  constructor() {
    effect((onCleanup) => {
      if (!this.refreshing()) {
        this._showRefresh.set(false);
        return;
      }
      const timer = setTimeout(() => this._showRefresh.set(true), ListState.REVEAL_MS);
      onCleanup(() => clearTimeout(timer));
    });
  }
}
