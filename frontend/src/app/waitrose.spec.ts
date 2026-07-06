import { afterEach, describe, expect, it } from 'vitest';

import { Waitrose } from './waitrose';

// The native bridge lives on window; fake it per-test.
interface TestWin {
  WaitroseBridge?: unknown;
  __waitroseResolve?: (id: string, res: unknown) => void;
  __waitroseConnected?: () => void;
}
const w = window as unknown as TestWin;

function fakeBridge(over: Record<string, unknown>) {
  w.WaitroseBridge = {
    available: () => true,
    connect: () => {},
    fetchProduct: () => {},
    searchByName: () => {},
    ...over,
  };
}

describe('Waitrose bridge service', () => {
  afterEach(() => {
    delete w.WaitroseBridge;
    delete w.__waitroseResolve;
    delete w.__waitroseConnected;
  });

  it('available is false in a plain browser (no bridge)', () => {
    expect(new Waitrose().available).toBe(false);
  });

  it('available is true when the native bridge is present', () => {
    fakeBridge({});
    expect(new Waitrose().available).toBe(true);
  });

  it('search resolves the candidates delivered via __waitroseResolve', async () => {
    let id = '';
    fakeBridge({ searchByName: (_q: string, rid: string) => (id = rid) });
    const svc = new Waitrose();
    const p = svc.search('milk');
    w.__waitroseResolve!(id, {
      ok: true,
      candidates: [{ lineNumber: '062593', name: 'Milk', image_url: 'x' }],
    });
    await expect(p).resolves.toEqual([{ lineNumber: '062593', name: 'Milk', image_url: 'x' }]);
  });

  it('fetchProduct rejects when the bridge returns an error', async () => {
    let id = '';
    fakeBridge({ fetchProduct: (_ln: string, rid: string) => (id = rid) });
    const svc = new Waitrose();
    const p = svc.fetchProduct('062593');
    w.__waitroseResolve!(id, { ok: false, error: 'no token' });
    await expect(p).rejects.toThrow('no token');
  });

  it('connect calls the bridge and resolves when the overlay closes', async () => {
    let opened = false;
    fakeBridge({ connect: () => (opened = true) });
    const svc = new Waitrose();
    const p = svc.connect();
    expect(opened).toBe(true);
    w.__waitroseConnected!();
    await expect(p).resolves.toBeUndefined();
  });

  it('methods reject when there is no bridge', async () => {
    await expect(new Waitrose().search('x')).rejects.toThrow(/only available in the app/);
  });
});
