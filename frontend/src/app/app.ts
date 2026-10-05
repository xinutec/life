import { Component, effect, inject, signal } from '@angular/core';
import { Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatMenuModule } from '@angular/material/menu';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatTooltipModule } from '@angular/material/tooltip';
import { Dialogs, Scaffold } from '@xinutec/ui-scaffold';

import { assertNever, classifyApiError, isNotFound } from './shared/api-error';
import { Alerts } from './shared/alerts';
import { Feedback } from './shared/feedback';
import { ScannerDialog } from './features/scanner/scanner-dialog';
import { LifeApi } from './life-api';
import { isRecord } from './shared/narrow';
import { ConnectionStatus, Me } from './models';
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

/** Every ConnectionStatus; the Record proves the list complete. */
const CONNECTION_STATUSES: string[] = Object.keys({
  active: true,
  needs_reauth: true,
  not_linked: true,
} satisfies Record<ConnectionStatus, true>);

/** Checked, not asserted: the cache can hold a shape from versions ago. */
function isMe(v: unknown): v is Me {
  if (!isRecord(v)) return false;
  const m = v;
  return (
    typeof m['userId'] === 'string' &&
    typeof m['displayName'] === 'string' &&
    typeof m['avatarUrl'] === 'string' &&
    typeof m['nextcloud'] === 'string' &&
    CONNECTION_STATUSES.includes(m['nextcloud'])
  );
}

/** The last-known identity, so the app opens offline rather than at sign-in. */
function loadCachedMe(): Me | null {
  try {
    const raw = localStorage.getItem(ME_CACHE_KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    return isMe(parsed) ? parsed : null;
  } catch {
    return null;
  }
}
function cacheMe(m: Me | null): void {
  try {
    if (m) localStorage.setItem(ME_CACHE_KEY, JSON.stringify(m));
    else localStorage.removeItem(ME_CACHE_KEY);
  } catch {}
}

@Component({
  selector: 'app-root',
  templateUrl: './app.html',
  styleUrl: './app.scss',
  imports: [
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    MatButtonModule,
    MatCardModule,
    MatIconModule,
    MatMenuModule,
    MatProgressBarModule,
    Scaffold,
    MatTooltipModule,
  ],
})
export class App {
  private api = inject(LifeApi);
  private telemetry = inject(Telemetry);
  private swUpdates = inject(SwUpdates);
  private wellbeingReminder = inject(WellbeingReminder);
  private auth = inject(AuthState);
  private dialog = inject(Dialogs);
  private feedback = inject(Feedback);
  private router = inject(Router);
  protected readonly alerts = inject(Alerts);
  protected readonly sync = inject(SyncStatus);

  private readonly cached = loadCachedMe();
  readonly me = signal<Me | null>(this.cached);
  /** Only a cold start with no cached identity blocks on a loader. */
  readonly loading = signal(this.cached === null);
  /** A background /api/me refresh has outlived the reveal delay. */
  readonly refreshing = signal(false);
  private refreshTimer: ReturnType<typeof setTimeout> | null = null;
  /** /api/me failed for a reason other than auth, with no cached identity:
   *  show "offline", not "sign in". */
  readonly offline = signal(false);
  readonly avatarError = signal(false);

  readonly nav: NavItem[] = [
    { path: '/today', icon: 'today', label: 'Today' },
    { path: '/shopping', icon: 'shopping_cart', label: 'Buy' },
    { path: '/inventory', icon: 'kitchen', label: 'Inventory' },
    { path: '/recipes', icon: 'menu_book', label: 'Recipes' },
    { path: '/todo', icon: 'checklist', label: 'To-do' },
  ];

  readonly more: NavItem[] = [
    { path: '/wellbeing', icon: 'mood', label: 'Wellbeing' },
    { path: '/emotions', icon: 'calendar_month', label: 'Emotion calendar' },
    { path: '/house', icon: 'home', label: 'House' },
    { path: '/items', icon: 'inventory_2', label: 'All items' },
    { path: '/trash', icon: 'restore_from_trash', label: 'Recently deleted' },
    { path: '/conflicts', icon: 'compare_arrows', label: 'Sync conflicts' },
    { path: '/settings', icon: 'settings', label: 'Settings' },
  ];

  constructor() {
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
            this.me.set(null);
            cacheMe(null);
            break;
          case 'offline':
          case 'server':
            // Not a logout: keep the cached identity.
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

  /** Show the refresh line only past 400 ms, so a fast refresh never flashes it. */
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

  retry(): void {
    window.location.reload();
  }

  // Read each endpoint once so the service worker caches it for offline use.
  private warmOfflineCache(): void {
    const ignore = { error: () => {} };
    this.api.items().subscribe(ignore);
    this.api.locations().subscribe(ignore);
    this.api.recipes().subscribe(ignore);
    this.api.cookable().subscribe(ignore);
    this.api.house().subscribe(ignore);
  }

  /** Scan a barcode and open its product page. Every outcome is announced:
   *  silence reads as a broken scanner. */
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
