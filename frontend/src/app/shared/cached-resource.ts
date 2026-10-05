import { Signal, signal } from '@angular/core';
import { Observable, Subject, of } from 'rxjs';
import { catchError, map, switchMap, tap } from 'rxjs/operators';

/** A cached server GET shared by every view. `refresh()` keeps the old value
 *  on screen, so it is safe on every view entry and after every write. */
export class CachedResource<T> {
  private readonly _value = signal<T | null>(null);
  private readonly _loaded = signal(false);
  private readonly _error = signal(false);
  private readonly _refreshing = signal(false);
  private readonly trigger$ = new Subject<void>();

  /** Null before the first successful load. */
  readonly value: Signal<T | null> = this._value.asReadonly();
  /** The first load has settled, either way. */
  readonly loaded: Signal<boolean> = this._loaded.asReadonly();
  /** A load failed with nothing cached to show instead. */
  readonly error: Signal<boolean> = this._error.asReadonly();
  readonly refreshing: Signal<boolean> = this._refreshing.asReadonly();

  constructor(loader: () => Observable<T>) {
    // switchMap: a newer refresh cancels an older one.
    this.trigger$
      .pipe(
        tap(() => {
          this._refreshing.set(true);
          this._error.set(false);
        }),
        switchMap(() =>
          loader().pipe(
            map((value) => ({ ok: true as const, value })),
            catchError(() => of({ ok: false as const })),
          ),
        ),
      )
      .subscribe((r) => {
        if (r.ok) this._value.set(r.value);
        this._loaded.set(true);
        this._refreshing.set(false);
        this._error.set(!r.ok && this._value() === null);
      });
  }

  refresh(): void {
    this.trigger$.next();
  }

  /** Update the cached value at once after a local change. */
  patch(update: (current: T | null) => T): void {
    this._value.set(update(this._value()));
  }
}
