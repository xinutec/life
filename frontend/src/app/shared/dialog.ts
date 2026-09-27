import { Component, input } from '@angular/core';
import { MatDialogModule } from '@angular/material/dialog';

/** A Material dialog shell: `<app-dialog title="…">` with the body as content
 *  and `dialogActions` buttons right-aligned.
 *
 *  Material's dialog parts fail silently when composed wrong (an inert title, a
 *  body without padding, an outline field's label clipped). This emits the right
 *  structure and reserves the label's room on `.dialog-body`. */
@Component({
  selector: 'app-dialog',
  templateUrl: './dialog.html',
  styleUrl: './dialog.scss',
  imports: [MatDialogModule],
})
export class Dialog {
  readonly title = input.required<string>();
}
