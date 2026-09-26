import { Signal, signal } from '@angular/core';
import { Observable, Subject, of } from 'rxjs';
import { catchError, map, switchMap, tap } from 'rxjs/operators';

/** A root-scoped, cached read model over a server GET. As a singleton it keeps
 *  a tab's last data across route switches (no blank placeholder on return) and
 *  lets views reading the same resource share one fetch.
 *
 *  `refresh()` re-fetches in the background without clearing the value, so it is
 *  safe to call on every view entry and after every mutation.
 *
 *  ```ts
 *  @Injectable({ providedIn: 'root' })
 *  export class ItemsStore extends CachedResource<Item[]> {
 *    constructor() { const api = inject(LifeApi); super(() => api.items()); }
 *  }
 *  ```
 */
export class CachedResource<T> {
  private readonly _value = signal<T | null>(null);
  private readonly _loaded = signal(false);
  private readonly _error = signal(false);
  private readonly _refreshing = signal(false);
  private readonly trigger$ = new Subject<void>();

  /** Last fetched value, or `null` before the first successful load. */
  readonly value: Signal<T | null> = this._value.asReadonly();
  /** True once a first load has settled (success OR failure). Gates the initial
   *  loading placeholder; stays true across refreshes, so a revisit never shows
   *  the placeholder again. */
  readonly loaded: Signal<boolean> = this._loaded.asReadonly();
  /** True only when a load failed AND there is no cached value to show instead.
   *  A failed background refresh over existing data is NOT an error — the data
   *  stands (offline keeps working); only a first load with nothing to fall back
   *  on surfaces a retry. */
  readonly error: Signal<boolean> = this._error.asReadonly();
  /** A fetch is in flight (a first load or a background revisit re-fetch). */
  readonly refreshing: Signal<boolean> = this._refreshing.asReadonly();

  constructor(loader: () => Observable<T>) {
    // A single long-lived pipeline; switchMap cancels an in-flight request when
    // refresh() fires again (rapid revisits / mutations don't race). This is a
    // root singleton, so the subscription is meant to live for the app's life.
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
        // Error only when the fetch failed and we have nothing cached to show.
        this._error.set(!r.ok && this._value() === null);
      });
  }

  /** Fetch (or re-fetch). The current value stays visible throughout. */
  refresh(): void {
    this.trigger$.next();
  }

  /** Optimistically patch the cached value after a local mutation (e.g. removing
   *  a restored/resolved row), so the UI updates instantly without waiting for a
   *  refetch. A background `refresh()` still reconciles with the server. */
  patch(update: (current: T | null) => T): void {
    this._value.set(update(this._value()));
  }
}
