import { TodoPriority, TodoType } from '../../models';
import { keysOf } from '../../shared/narrow';

/** In picker order. A total Record, so a new type must be named. */
const TYPE_META: Record<TodoType, { label: string; icon: string }> = {
  purchase: { label: 'Purchase', icon: 'shopping_bag' },
  call: { label: 'Call', icon: 'call' },
  appointment: { label: 'Appointment', icon: 'event' },
  admin: { label: 'Admin', icon: 'description' },
  task: { label: 'Task', icon: 'task_alt' },
};

export const TODO_TYPES: readonly { value: TodoType; label: string; icon: string }[] = keysOf(
  TYPE_META,
).map((value) => ({ value, ...TYPE_META[value] }));

/** Highest first. */
const PRIORITY_LABEL: Record<TodoPriority, string> = { high: 'High', medium: 'Medium', low: 'Low' };

export const PRIORITIES: readonly { value: TodoPriority; label: string }[] = keysOf(
  PRIORITY_LABEL,
).map((value) => ({ value, label: PRIORITY_LABEL[value] }));

const PRIO_RANK: Record<TodoPriority, number> = { high: 0, medium: 1, low: 2 };
/** Unset last. */
export const prioRank = (p: TodoPriority | null): number => (p ? PRIO_RANK[p] : 3);
