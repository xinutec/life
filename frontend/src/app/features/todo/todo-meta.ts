import { TodoPriority, TodoType } from '../../models';
import { keysOf } from '../../shared/narrow';

/** Each to-do type's label and Material icon, in picker order. A total
 *  `Record`, so a type the backend adds fails to compile here rather than going
 *  missing from every picker. One table for list, detail and add sheet. */
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

/** Highest first, the order the chips show them; total for the same reason. */
const PRIORITY_LABEL: Record<TodoPriority, string> = { high: 'High', medium: 'Medium', low: 'Low' };

export const PRIORITIES: readonly { value: TodoPriority; label: string }[] = keysOf(
  PRIORITY_LABEL,
).map((value) => ({ value, label: PRIORITY_LABEL[value] }));

const PRIO_RANK: Record<TodoPriority, number> = { high: 0, medium: 1, low: 2 };
/** Sort rank: high → medium → low → unset. */
export const prioRank = (p: TodoPriority | null): number => (p ? PRIO_RANK[p] : 3);
