import { Component, input } from '@angular/core';
import { MatDialogModule } from '@angular/material/dialog';

/** The one correct Material dialog shell: a title, a scrollable body, and a
 *  right-aligned action bar — assembled the single valid way so a feature can't
 *  get the composition wrong. Project the body as default content and the
 *  buttons with a `dialogActions` attribute:
 *
 *    <app-dialog title="Find on Waitrose">
 *      <mat-form-field>…</mat-form-field>
 *      <button dialogActions mat-button (click)="close()">Cancel</button>
 *    </app-dialog>
 *
 *  Why this exists at all: Material's dialog is a loose composition of attribute
 *  directives (`mat-dialog-title` / `-content` / `-actions`) over permissive
 *  primitives. Omit `MatDialogModule` and the title is a silently-inert
 *  attribute; put the body outside `mat-dialog-content` and it loses its padding;
 *  leave an `appearance="outline"` field as the first child and Material's zeroed
 *  top padding shears its floating label. None of those error — they just render
 *  wrong. This shell emits the correct structure every time and reserves the top
 *  room an outline label needs on its OWN class (`.dialog-body`), so nothing has
 *  to win a specificity fight against Material's stylesheet. */
@Component({
  selector: 'app-dialog',
  templateUrl: './dialog.html',
  styleUrl: './dialog.scss',
  imports: [MatDialogModule],
})
export class Dialog {
  readonly title = input.required<string>();
}
