import { Injectable, inject } from '@angular/core';
import { MatSnackBar } from '@angular/material/snack-bar';

/** The app's snackbars, so every screen words and times them alike. */
@Injectable({ providedIn: 'root' })
export class Feedback {
  private snack = inject(MatSnackBar);

  error(message = 'Something went wrong — are you online?'): void {
    this.snack.open(message, 'OK', { duration: 4000 });
  }

  notify(message: string): void {
    this.snack.open(message, undefined, { duration: 2500 });
  }

  /** `onCommit` runs when the bar closes without an Undo. */
  undo(message: string, onUndo: () => void, onCommit?: () => void): void {
    const ref = this.snack.open(message, 'Undo', { duration: 6000 });
    let undone = false;
    ref.onAction().subscribe(() => {
      undone = true;
      onUndo();
    });
    ref.afterDismissed().subscribe(() => {
      if (!undone) onCommit?.();
    });
  }
}
