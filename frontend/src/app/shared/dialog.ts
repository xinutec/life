import { Component, input } from '@angular/core';
import { MatDialogModule } from '@angular/material/dialog';

/** A Material dialog with the parts composed right: they fail silently otherwise. */
@Component({
  selector: 'app-dialog',
  templateUrl: './dialog.html',
  styleUrl: './dialog.scss',
  imports: [MatDialogModule],
})
export class Dialog {
  readonly title = input.required<string>();
}
