import { Injectable, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { BehaviorSubject, of } from 'rxjs';
import { catchError, map, switchMap } from 'rxjs/operators';

import { LifeApi } from '../../life-api';
import { daysBetween, localDay } from '../../shared/civil-day';
import { ItemsStore, LocationsStore, RecipesStore } from '../../stores/catalog';
import { LinkKind, TargetKind } from '../../models';
import { ShoppingDoc, ShoppingStore } from '../../sync/shopping-store';
import { TodoLinkDoc, TodoLinkStore } from '../../sync/todo-link-store';
import { TodoDoc, TodoStore } from '../../sync/todo-store';

export interface LinkTarget {
  kind: TargetKind;
  ref: string;
  label: string;
  icon: string;
}

export interface ResolvedLink {
  ulid: string; // the edge's own ulid
  linkKind: LinkKind;
  target: LinkTarget;
}

export type TodoState = 'done' | 'blocked' | 'waiting' | 'ready' | 'open';

/** Deadline pressure, apart from `TodoState`. */
export type Urgency = 'overdue' | 'today' | 'soon' | 'none';

export const URGENCY_RANK: Record<Urgency, number> = { overdue: 0, today: 1, soon: 2, none: 3 };

export interface DueChip {
  label: string;
  cls: 'overdue' | 'due-soon';
}

const TARGET_ICON: Record<TargetKind, string> = {
  todo: 'task_alt',
  item: 'inventory_2',
  recipe: 'menu_book',
  room: 'meeting_room',
  shopping: 'shopping_cart',
  place: 'place',
};

/** To-dos, their links and everything a link can point at: resolves labels,
 *  searches, and decides ready and blocked. */
@Injectable({ providedIn: 'root' })
export class TodoGraph {
  private todos = inject(TodoStore);
  private linkStore = inject(TodoLinkStore);
  private shopping = inject(ShoppingStore);
  private api = inject(LifeApi);

  // A signal, updated at midnight and on regaining focus.
  private readonly _today = signal(localDay());
  readonly today = this._today.asReadonly();

  constructor() {
    const refresh = () => this._today.set(localDay());
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') refresh();
      });
    }
    const scheduleMidnight = () => {
      const now = new Date();
      const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 30);
      setTimeout(() => {
        refresh();
        scheduleMidnight();
      }, next.getTime() - now.getTime());
    };
    scheduleMidnight();
  }

  daysUntil(iso: string): number {
    return daysBetween(this.today(), iso) ?? Number.NaN;
  }

  /** Null when done, undated, or due more than 3 days out. */
  dueChip(todo: TodoDoc): DueChip | null {
    const u = this.urgencyOf(todo);
    if (u === 'none' || !todo.due) return null;
    const d = this.daysUntil(todo.due);
    if (u === 'overdue')
      return { label: d === -1 ? 'overdue 1d' : `overdue ${-d}d`, cls: 'overdue' };
    if (u === 'today') return { label: 'due today', cls: 'overdue' };
    return { label: d === 1 ? 'due tomorrow' : `due in ${d}d`, cls: 'due-soon' };
  }

  urgencyOf(todo: TodoDoc): Urgency {
    if (todo.status === 'done' || !todo.due) return 'none';
    const d = this.daysUntil(todo.due);
    if (d < 0) return 'overdue';
    if (d === 0) return 'today';
    if (d <= 3) return 'soon';
    return 'none';
  }

  readonly todoItems = toSignal(this.todos.items$, { initialValue: [] as TodoDoc[] });
  readonly links = toSignal(this.linkStore.links$, { initialValue: [] as TodoLinkDoc[] });
  private readonly shoppingItems = toSignal(this.shopping.items$, {
    initialValue: [] as ShoppingDoc[],
  });

  // Rooms come from the house scene, which has no shared store.
  private itemsStore = inject(ItemsStore);
  private recipesStore = inject(RecipesStore);
  private placesStore = inject(LocationsStore);

  private readonly items = computed(() => this.itemsStore.value() ?? []);
  private readonly recipes = computed(() => this.recipesStore.value() ?? []);
  private readonly places = computed(() => this.placesStore.value() ?? []);

  private readonly roomsRefresh$ = new BehaviorSubject<void>(undefined);
  private readonly rooms = toSignal(
    this.roomsRefresh$.pipe(
      switchMap(() =>
        this.api.house().pipe(
          map((h) => (h.rooms ?? []).map((r) => r.name).filter((n): n is string => !!n)),
          catchError(() => of([] as string[])),
        ),
      ),
    ),
    { initialValue: [] as string[] },
  );

  /** So something added since can be linked. */
  refreshCatalogs(): void {
    this.itemsStore.refresh();
    this.recipesStore.refresh();
    this.placesStore.refresh();
    this.roomsRefresh$.next(undefined);
  }

  readonly catalog = computed<LinkTarget[]>(() => {
    const out: LinkTarget[] = [];
    for (const t of this.todoItems())
      out.push({ kind: 'todo', ref: t.ulid, label: t.title, icon: TARGET_ICON.todo });
    for (const i of this.items())
      out.push({ kind: 'item', ref: String(i.id), label: i.name, icon: TARGET_ICON.item });
    for (const r of this.recipes())
      out.push({ kind: 'recipe', ref: String(r.id), label: r.name, icon: TARGET_ICON.recipe });
    for (const name of this.rooms())
      out.push({ kind: 'room', ref: name, label: name, icon: TARGET_ICON.room });
    for (const s of this.shoppingItems())
      out.push({ kind: 'shopping', ref: s.ulid, label: s.name, icon: TARGET_ICON.shopping });
    for (const p of this.places())
      out.push({ kind: 'place', ref: String(p.id), label: p.name, icon: TARGET_ICON.place });
    return out;
  });

  private readonly byKey = computed(() => {
    const m = new Map<string, LinkTarget>();
    for (const t of this.catalog()) m.set(t.kind + ':' + t.ref, t);
    return m;
  });

  private readonly todoByUlid = computed(() => {
    const m = new Map<string, TodoDoc>();
    for (const t of this.todoItems()) m.set(t.ulid, t);
    return m;
  });

  resolve(kind: TargetKind, ref: string): LinkTarget {
    return (
      this.byKey().get(kind + ':' + ref) ?? {
        kind,
        ref,
        label: '(deleted)',
        icon: TARGET_ICON[kind],
      }
    );
  }

  search(query: string, excludeTodo?: string): LinkTarget[] {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return this.catalog()
      .filter((t) => !(t.kind === 'todo' && t.ref === excludeTodo))
      .filter((t) => t.label.toLowerCase().includes(q))
      .slice(0, 25);
  }

  outgoing(todoUlid: string): ResolvedLink[] {
    return this.links()
      .filter((l) => l.from === todoUlid)
      .map((l) => ({
        ulid: l.ulid,
        linkKind: l.kind,
        target: this.resolve(l.targetKind, l.targetRef),
      }));
  }

  incoming(todoUlid: string): { ulid: string; linkKind: LinkKind; source: LinkTarget }[] {
    return this.links()
      .filter((l) => l.targetKind === 'todo' && l.targetRef === todoUlid)
      .map((l) => ({ ulid: l.ulid, linkKind: l.kind, source: this.resolve('todo', l.from) }));
  }

  /** The open dependencies: only to-dos and Buy rows have a done state, so only
   *  they can block. */
  blockers(todoUlid: string): { ulid: string; title: string }[] {
    const todoMap = this.todoByUlid();
    const shopMap = new Map(this.shoppingItems().map((s) => [s.ulid, s] as const));
    const out: { ulid: string; title: string }[] = [];
    for (const l of this.links()) {
      if (l.from !== todoUlid || l.kind !== 'depends_on') continue;
      if (l.targetKind === 'todo') {
        const t = todoMap.get(l.targetRef);
        if (t && t.status !== 'done') out.push({ ulid: t.ulid, title: t.title });
      } else if (l.targetKind === 'shopping') {
        const s = shopMap.get(l.targetRef);
        if (s && !s.done) out.push({ ulid: s.ulid, title: s.name });
      }
    }
    return out;
  }

  /** done, then blocked, waiting (a future start), ready (has dependencies, all
   *  met), open. */
  statusOf(todo: TodoDoc): TodoState {
    if (todo.status === 'done') return 'done';
    if (this.blockers(todo.ulid).length > 0) return 'blocked';
    if (todo.notBefore && this.daysUntil(todo.notBefore) > 0) return 'waiting';
    const hasDeps = this.links().some(
      (l) =>
        l.from === todo.ulid &&
        l.kind === 'depends_on' &&
        (l.targetKind === 'todo' || l.targetKind === 'shopping'),
    );
    return hasDeps ? 'ready' : 'open';
  }

  linkCount(todoUlid: string): number {
    return this.links().filter(
      (l) => l.from === todoUlid || (l.targetKind === 'todo' && l.targetRef === todoUlid),
    ).length;
  }

  add(input: { from: string; kind: LinkKind; targetKind: TargetKind; targetRef: string }): void {
    void this.linkStore.add(input);
  }

  removeLink(edgeUlid: string): void {
    void this.linkStore.remove(edgeUlid);
  }

  removeLinksForTodo(todoUlid: string): void {
    void this.linkStore.removeForTodo(todoUlid);
  }
}
