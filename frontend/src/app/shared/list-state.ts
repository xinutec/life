import { Component, effect, input, output, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';

/** The loading / error / empty line above a list. */
@Component({
  selector: 'app-list-state',
  templateUrl: './list-state.html',
  styleUrl: './list-state.scss',
  imports: [MatButtonModule, MatIconModule, MatProgressBarModule],
})
export class ListState {
  readonly loading = input(false);
  readonly error = input(false);
  readonly empty = input(false);
  /** A background refresh; shown as a bar only past REVEAL_MS. */
  readonly refreshing = input(false);

  readonly emptyText = input('Nothing here yet.');
  readonly emptyIcon = input<string | null>(null);
  readonly errorText = input('Couldn’t load — are you online?');

  readonly retry = output<void>();

  private readonly _showRefresh = signal(false);
  protected readonly showRefresh = this._showRefresh.asReadonly();
  /** As the app shell's. */
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
