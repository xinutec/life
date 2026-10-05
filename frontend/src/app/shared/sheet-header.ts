import { Component, input, output } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';

/** Every bottom sheet's title and Close button. */
@Component({
  selector: 'app-sheet-header',
  templateUrl: './sheet-header.html',
  styleUrl: './sheet-header.scss',
  imports: [MatButtonModule, MatIconModule],
})
export class SheetHeader {
  readonly title = input.required<string>();
  readonly closed = output<void>();
}
