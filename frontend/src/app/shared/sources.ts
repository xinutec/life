import type { Source } from '../models';

/** A source's display name. No `default` arm, so a new source must be named. */
export function sourceLabel(source: Source | null): string {
  switch (source) {
    case 'off':
      return 'Open Food Facts';
    case 'asda':
      return 'Asda';
    case 'waitrose':
      return 'Waitrose';
    case 'user':
      return 'added by you';
    case null:
      return '';
  }
}
