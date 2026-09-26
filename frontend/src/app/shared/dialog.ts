import { Component, input } from '@angular/core';
import { MatDialogModule } from '@angular/material/dialog';

/** A Material dialog shell: title, scrollable body, right-aligned actions.
 *
 *    <app-dialog title="Find a product">
 *      <mat-form-field>…</mat-form-field>
 *      <button dialogActions mat-button (click)="close()">Cancel</button>
 *    </app-dialog>
 *
 *  Material's dialog parts fail silently when composed wrong: a title without
 *  `MatDialogModule` is inert, a body outside `mat-dialog-content` loses its
 *  padding, and an outline field first in the body gets its label clipped.
 *  This emits the right structure and reserves the label's room on its own
 *  `.dialog-body` class, avoiding a specificity fight with Material. */
@Component({
  selector: 'app-dialog',
  templateUrl: './dialog.html',
  styleUrl: './dialog.scss',
  imports: [MatDialogModule],
})
export class Dialog {
  readonly title = input.required<string>();
}
