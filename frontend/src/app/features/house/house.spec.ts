import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { House } from './house';

function setup() {
  TestBed.configureTestingModule({
    imports: [House],
    providers: [provideHttpClient(), provideHttpClientTesting()],
  });
  const fixture = TestBed.createComponent(House);
  // No detectChanges(): ngAfterViewInit would boot a WebGL renderer the test
  // environment doesn't have. The load states are exercised via load() directly.
  const http = TestBed.inject(HttpTestingController);
  return { cmp: fixture.componentInstance, http };
}

describe('House load states', () => {
  it('renders a server failure as the error state, never as "no layout yet"', () => {
    const { cmp, http } = setup();
    cmp.load();
    expect(cmp.loading()).toBe(true);
    http.expectOne('/api/house').flush('boom', { status: 500, statusText: 'Server Error' });
    expect(cmp.loading()).toBe(false);
    expect(cmp.error()).toBe(true);
    expect(cmp.empty()).toBe(false);
  });

  it('renders offline as the error state too', () => {
    const { cmp, http } = setup();
    cmp.load();
    http.expectOne('/api/house').error(new ProgressEvent('error'));
    expect(cmp.error()).toBe(true);
    expect(cmp.empty()).toBe(false);
  });

  it('renders a 404 — genuinely no scene — as the empty state', () => {
    const { cmp, http } = setup();
    cmp.load();
    http.expectOne('/api/house').flush('missing', { status: 404, statusText: 'Not Found' });
    expect(cmp.empty()).toBe(true);
    expect(cmp.error()).toBe(false);
  });

  it('retrying after an error re-fetches', () => {
    const { cmp, http } = setup();
    cmp.load();
    http.expectOne('/api/house').flush('boom', { status: 500, statusText: 'Server Error' });
    cmp.load();
    expect(cmp.error()).toBe(false);
    expect(cmp.loading()).toBe(true);
    http.expectOne('/api/house');
  });
});
