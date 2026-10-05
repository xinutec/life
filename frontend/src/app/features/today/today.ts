import { Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { RouterLink } from '@angular/router';
import { MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { Sheets } from '@xinutec/ui-scaffold';

import { map } from 'rxjs';

import { nextCollections } from '../../bins';
import { expiryInfo } from '../../expiry';
import { Feedback } from '../../shared/feedback';
import { BinsStore, ItemsStore } from '../../stores/catalog';
import { ListState } from '../../shared/list-state';
import { WellbeingCheckin } from '../../shared/wellbeing-checkin';
import { ShoppingDoc, ShoppingStore } from '../../sync/shopping-store';
import { TodoDoc, TodoStore } from '../../sync/todo-store';
import { prioRank } from '../todo/todo-meta';
import { TodoDetail } from '../todo/todo-detail';
import { WellbeingEntry } from '../wellbeing/wellbeing-entry';
import { TodoGraph, URGENCY_RANK } from '../todo/todo-graph';

const READY = { label: 'ready', cls: 'ready' };

interface Attention {
  todo: TodoDoc;
  chip: { label: string; cls: string };
}

/** "What needs me now": a check-in, pressing to-dos, expiring food, bins. */
@Component({
  selector: 'app-today',
  templateUrl: './today.html',
  styleUrl: './today.scss',
  imports: [
    RouterLink,
    MatBottomSheetModule,
    MatButtonModule,
    MatCardModule,
    MatCheckboxModule,
    MatIconModule,
    MatListModule,
    ListState,
    WellbeingCheckin,
  ],
})
export class Today {
  private shopping = inject(ShoppingStore);
  private todos = inject(TodoStore);
  private graph = inject(TodoGraph);
  private sheet = inject(Sheets);
  private feedback = inject(Feedback);
  private itemsStore = inject(ItemsStore);
  private binsStore = inject(BinsStore);

  private readonly items = computed(() => this.itemsStore.value() ?? []);
  private readonly shoppingItems = toSignal(this.shopping.items$, {
    initialValue: [] as ShoppingDoc[],
  });

  /** Until the to-dos load, not a false "Nothing pressing". */
  readonly loaded = toSignal(this.todos.items$.pipe(map(() => true)), { initialValue: false });

  constructor() {
    this.itemsStore.refresh();
    this.binsStore.refresh();
  }

  /** Overdue, due soon, or ready; never blocked or waiting. Capped at five. */
  readonly attention = computed<Attention[]>(() => {
    return this.graph
      .todoItems()
      .filter((t) => t.status !== 'done')
      .map((todo) => ({
        todo,
        state: this.graph.statusOf(todo),
        urgency: this.graph.urgencyOf(todo),
      }))
      .filter((x) => x.state !== 'waiting' && x.state !== 'blocked')
      .filter((x) => x.urgency !== 'none' || x.state === 'ready')
      .sort(
        (a, b) =>
          URGENCY_RANK[a.urgency] - URGENCY_RANK[b.urgency] ||
          prioRank(a.todo.priority) - prioRank(b.todo.priority) ||
          (a.todo.due ?? '9999-99-99').localeCompare(b.todo.due ?? '9999-99-99'),
      )
      .slice(0, 5)
      .map((x) => ({ todo: x.todo, chip: this.graph.dueChip(x.todo) ?? READY }));
  });

  /** Two: "is it tonight, and if not, when". */
  readonly bins = computed(() => nextCollections(this.binsStore.value() ?? []).slice(0, 2));

  readonly expiring = computed(() => {
    return this.items()
      .flatMap((item) =>
        item.expiry
          ? [
              {
                item,
                expiry: item.expiry,
                info: expiryInfo(item.expiry, item.expiry_precision),
              },
            ]
          : [],
      )
      .filter((x) => x.info.cls !== 'ok')
      .sort((a, b) => a.expiry.localeCompare(b.expiry))
      .slice(0, 5);
  });

  readonly buyCount = computed(() => this.shoppingItems().filter((i) => !i.done).length);

  /** Rows here are never blocked. */
  complete(todo: TodoDoc): void {
    void this.todos.setStatus(todo.ulid, 'done');
    this.feedback.undo(`Done: ${todo.title}`, () => void this.todos.setStatus(todo.ulid, 'open'));
  }

  open(todo: TodoDoc): void {
    this.sheet.open(TodoDetail, { data: { ulid: todo.ulid } });
  }

  addDetail(ulid: string): void {
    this.sheet.open(WellbeingEntry, { data: { ulid } });
  }
}
