import { Component, effect, inject, signal } from '@angular/core';
import { Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { MatBadgeModule } from '@angular/material/badge';
import { MatDialog } from '@angular/material/dialog';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatMenuModule } from '@angular/material/menu';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatToolbarModule } from '@angular/material/toolbar';
import { MatTooltipModule } from '@angular/material/tooltip';

import { assertNever, classifyApiError, isNotFound } from './shared/api-error';
import { Alerts } from './shared/alerts';
import { Feedback } from './shared/feedback';
import { ScannerDialog } from './features/scanner/scanner-dialog';
import { LifeApi } from './life-api';
import { Me } from './models';
import { SwUpdates } from './sw-updates';
import { Telemetry } from './telemetry';
import { WellbeingReminder } from './shared/wellbeing-reminder';
import { AuthState } from './sync/auth-state';
import { SyncStatus } from './sync/sync-status';

interface NavItem {
  path: string;
  icon: string;
  label: string;
}

const ME_CACHE_KEY = 'life.me';

/** Last-known identity, cached so the app opens offline instead of showing the
 *  sign-in screen when it can't reach `/api/me`. Best-effort — storage may be
 *  unavailable (private mode, quota); a miss just falls back to the network. */
function loadCachedMe(): Me | null {
  try {
    const raw = localStorage.getItem(ME_CACHE_KEY);
    return raw ? (JSON.parse(raw) as Me) : null;
  } catch {
    return null;
  }
}
function cacheMe(m: Me | null): void {
  try {
    if (m) localStorage.setItem(ME_CACHE_KEY, JSON.stringify(m));
    else localStorage.removeItem(ME_CACHE_KEY);
  } catch {
    // Persisting identity is best-effort; ignore storage failures.
  }
}

@Component({
  selector: 'app-root',
  templateUrl: './app.html',
  styleUrl: './app.scss',
  imports: [
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    MatBadgeModule,
    MatButtonModule,
    MatCardModule,
    MatIconModule,
    MatMenuModule,
    MatProgressBarModule,
    MatToolbarModule,
    MatTooltipModule,
  ],
})
export class App {
  private api = inject(LifeApi);
  private telemetry = inject(Telemetry);
  private swUpdates = inject(SwUpdates);
  private wellbeingReminder = inject(WellbeingReminder);
  private auth = inject(AuthState);
  private dialog = inject(MatDialog);
  private feedback = inject(Feedback);
  private router = inject(Router);
  protected readonly alerts = inject(Alerts);
  protected readonly sync = inject(SyncStatus);

  private readonly cached = loadCachedMe();
  readonly me = signal<Me | null>(this.cached);
  /** Full-screen loader ONLY for a genuine cold start — no cached identity to
   *  show. A returning user renders their cached shell immediately and the
   *  /api/me refresh runs in the background (see `refreshing`), so there's no
   *  spinner-over-content flash. */
  readonly loading = signal(this.cached === null);
  /** A background /api/me refresh is in flight AND slow enough to be worth
   *  signalling — drives a thin, non-blocking progress line over the content
   *  (never instead of it). Revealed on a delay so a fast refresh stays silent. */
  readonly refreshing = signal(false);
  private refreshTimer: ReturnType<typeof setTimeout> | null = null;
  /** True when the initial /api/me call failed for a non-auth reason (offline or
   *  server) AND there was no cached identity to fall back to — drives a "you're
   *  offline" notice instead of the misleading "sign in" prompt. */
  readonly offline = signal(false);
  readonly avatarError = signal(false);

  // The frequent destinations live in the bottom tab bar.
  readonly nav: NavItem[] = [
    { path: '/today', icon: 'today', label: 'Today' },
    { path: '/shopping', icon: 'shopping_cart', label: 'Buy' },
    { path: '/inventory', icon: 'kitchen', label: 'Inventory' },
    { path: '/recipes', icon: 'menu_book', label: 'Recipes' },
    { path: '/todo', icon: 'checklist', label: 'To-do' },
  ];

  // Less-common destinations live behind the hamburger menu.
  readonly more: NavItem[] = [
    { path: '/wellbeing', icon: 'mood', label: 'Wellbeing' },
    { path: '/house', icon: 'home', label: 'House' },
    { path: '/items', icon: 'inventory_2', label: 'All items' },
    { path: '/trash', icon: 'restore_from_trash', label: 'Recently deleted' },
    { path: '/conflicts', icon: 'compare_arrows', label: 'Sync conflicts' },
    { path: '/settings', icon: 'settings', label: 'Settings' },
  ];

  constructor() {
    // Replication is usually the first to learn the session has lapsed — it polls,
    // and the user may be sitting on a page that asks the server for nothing. Take
    // its word for it and drop to the sign-in prompt, rather than leaving a shell
    // that looks signed in but can no longer sync (and, before this, silently
    // retried a doomed request every 5s for as long as the tab stayed open).
    effect(() => {
      if (this.auth.lost()) {
        this.me.set(null);
        cacheMe(null);
        this.loading.set(false);
        this.offline.set(false);
      }
    });

    this.swUpdates.start();
    this.telemetry.init();
    // Re-arm the daily wellbeing reminder from local check-ins on every open (a
    // no-op outside the Android app). Independent of the /api/me result — it reads
    // the offline store, so it works before (or without) the session confirming.
    this.wellbeingReminder.init();
    this.beginRefresh();
    this.api.me().subscribe({
      next: (m) => {
        this.me.set(m);
        cacheMe(m);
        this.offline.set(false);
        this.loading.set(false);
        this.endRefresh();
        this.warmOfflineCache();
        this.alerts.refreshConflicts();
      },
      error: (e) => {
        const f = classifyApiError(e);
        switch (f.kind) {
          case 'unauthenticated':
            // A genuinely expired/absent session — forget the cached identity
            // and fall through to the sign-in prompt.
            this.me.set(null);
            cacheMe(null);
            break;
          case 'offline':
          case 'server':
            // Couldn't confirm identity, but this is NOT a logout. Keep the
            // last-known `me` (already hydrated from cache) so the app opens; if
            // there's no cache, `offline` drives an "offline" notice rather than
            // telling the user to sign in when the real problem is connectivity.
            // Being offline is surfaced ambiently by the toolbar sync indicator
            // (SyncStatus watches navigator.onLine), so a shown-from-cache app
            // still signals its connectivity — no need to alarm here.
            this.offline.set(true);
            break;
          default:
            assertNever(f);
        }
        this.loading.set(false);
        this.endRefresh();
      },
    });
  }

  /** Arm the refresh cue: reveal it only if the fetch outlives the delay, so a
   *  fast refresh (or an instant offline failure) never flashes the line on and
   *  off. ~400ms is below where a wait starts to feel like waiting. */
  private beginRefresh(): void {
    this.refreshTimer = setTimeout(() => this.refreshing.set(true), 400);
  }
  private endRefresh(): void {
    if (this.refreshTimer !== null) {
      clearTimeout(this.refreshTimer);
      this.refreshTimer = null;
    }
    this.refreshing.set(false);
  }

  /** Retry the identity fetch from the offline notice — a reload re-runs the
   *  whole boot (the service worker still serves the shell). */
  retry(): void {
    window.location.reload();
  }

  // Fire the read endpoints once on login so the service worker caches them —
  // makes inventory/recipes/house viewable offline even if you went straight
  // underground without opening those tabs first. Fire-and-forget; the SW does
  // the caching, these responses are otherwise ignored.
  private warmOfflineCache(): void {
    const ignore = { error: () => {} };
    this.api.items().subscribe(ignore);
    this.api.locations().subscribe(ignore);
    this.api.recipes().subscribe(ignore);
    this.api.cookable().subscribe(ignore);
    this.api.house().subscribe(ignore);
  }

  /** The hamburger's "Scan a product": camera → barcode lookup → the product
   *  page. The standalone payoff-screen path — no form, no item, just "what is
   *  this thing on my shelf". Every outcome is announced (see the sheets'
   *  scan flows: silence reads as a broken scanner). */
  scanProduct(): void {
    this.dialog
      .open<ScannerDialog, unknown, string | null>(ScannerDialog, {
        panelClass: 'scanner-pane',
        ariaLabel: 'Barcode scanner',
      })
      .afterClosed()
      .subscribe((code) => {
        if (!code) return;
        this.api.lookupProduct(code).subscribe({
          next: (p) => void this.router.navigate(['/product', p.id]),
          error: (e: unknown) => {
            this.feedback.error(
              isNotFound(e) ? `No product found for ${code}.` : 'Lookup failed — are you online?',
            );
          },
        });
      });
  }

  signOut(): void {
    this.api.logout().subscribe(() => (window.location.href = '/'));
  }
}
