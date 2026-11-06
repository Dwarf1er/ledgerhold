import { describe, expect, it } from 'vitest';
import { links, parseHash } from './router.svelte';

describe('router', () => {
  it('parses every route', () => {
    expect(parseHash('')).toEqual({ name: 'overview' });
    expect(parseHash('#/')).toEqual({ name: 'overview' });
    expect(parseHash('#/settings')).toEqual({ name: 'settings' });
    expect(parseHash('#/import')).toEqual({ name: 'import', accountId: null });
    expect(parseHash('#/import?account=a%201')).toEqual({ name: 'import', accountId: 'a 1' });
    expect(parseHash('#/account/abc')).toEqual({ name: 'account', id: 'abc', tab: 'transactions' });
    expect(parseHash('#/account/abc?tab=balance')).toEqual({ name: 'account', id: 'abc', tab: 'balance' });
    expect(parseHash('#/account/abc?tab=nonsense').name).toBe('account');
    expect(parseHash('#/nope')).toEqual({ name: 'overview' });
  });

  it('links round-trip through the parser', () => {
    expect(parseHash(links.account('x y', 'imports'))).toEqual({ name: 'account', id: 'x y', tab: 'imports' });
    expect(parseHash(links.import('acc/1'))).toEqual({ name: 'import', accountId: 'acc/1' });
    expect(links.account('a')).toBe('#/account/a');
  });
});
