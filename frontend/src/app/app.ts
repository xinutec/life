import { Component, inject, signal } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { MatBadgeModule } from '@angular/material/badge';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatMenuModule } from '@angular/material/menu';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatToolbarModule } from '@angular/material/toolbar';
import { MatTooltipModule } from '@angular/material/tooltip';

import { assertNever, classifyApiError } from './shared/api-error';
import { Alerts } from './shared/alerts';
import { LifeApi } from './life-api';
import { Me } from './models';
import { SwUpdates } from './sw-updates';
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
  private swUpdates = inject(SwUpdates);
  protected readonly alerts = inject(Alerts);
  protected readonly sync = inject(SyncStatus);

  readonly me = signal<Me | null>(loadCachedMe());
  readonly loading = signal(true);
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
    this.swUpdates.start();
    this.api.me().subscribe({
      next: (m) => {
        this.me.set(m);
        cacheMe(m);
        this.offline.set(false);
        this.loading.set(false);
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
            this.offline.set(true);
            break;
          default:
            assertNever(f);
        }
        this.loading.set(false);
      },
    });
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

  signOut(): void {
    this.api.logout().subscribe(() => (window.location.href = '/'));
  }
}
