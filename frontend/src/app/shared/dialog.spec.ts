import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { Dialog } from './dialog';

/** Host that projects a body and an action, the way a feature dialog does. */
@Component({
  imports: [Dialog],
  template: `
    <app-dialog title="Find on Waitrose">
      <p class="body-marker">pick a product</p>
      <button dialogActions class="cancel">Cancel</button>
    </app-dialog>
  `,
})
class Host {}

@Component({
  imports: [Dialog],
  template: `<app-dialog title="Just a message"><p>hi</p></app-dialog>`,
})
class NoActionsHost {}

function render(host: typeof Host | typeof NoActionsHost): HTMLElement {
  const fixture = TestBed.createComponent(host);
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

describe('Dialog', () => {
  it('emits the canonical title → content → actions composition', () => {
    const el = render(Host);
    const title = el.querySelector('[mat-dialog-title]');
    const content = el.querySelector('mat-dialog-content');
    const actions = el.querySelector('mat-dialog-actions');
    expect(title?.textContent).toContain('Find on Waitrose');
    expect(content).toBeTruthy();
    expect(actions).toBeTruthy();
    // Adjacency is the load-bearing property: the title must DIRECTLY precede the
    // content (that's the selector Material uses to zero the top padding), and the
    // actions bar comes right after.
    expect(title?.nextElementSibling?.tagName).toBe('MAT-DIALOG-CONTENT');
    expect(content?.nextElementSibling?.tagName).toBe('MAT-DIALOG-ACTIONS');
  });

  it('projects the body inside .dialog-body (which carries the outline-label top room)', () => {
    const el = render(Host);
    const body = el.querySelector('mat-dialog-content .dialog-body');
    expect(body).toBeTruthy();
    expect(body?.querySelector('.body-marker')).toBeTruthy();
  });

  it('projects [dialogActions] into the action bar, not the body', () => {
    const el = render(Host);
    expect(el.querySelector('mat-dialog-actions .cancel')).toBeTruthy();
    expect(el.querySelector('.dialog-body .cancel')).toBeFalsy();
  });

  it('leaves the action bar empty when nothing is projected into it', () => {
    // `mat-dialog-actions:empty { display: none }` hides it; here we assert it has
    // no projected children so that CSS rule applies.
    const el = render(NoActionsHost);
    expect(el.querySelector('mat-dialog-actions')?.childElementCount).toBe(0);
  });
});
