import { describe, expect, it } from 'vitest';
import { extensionOf, suggestParser } from './parsers';

describe('suggestParser', () => {
  it('maps known banks and kinds', () => {
    expect(suggestParser({ institutionName: 'Desjardins', kind: 'checking' })).toBe('desjardins');
    expect(suggestParser({ institutionName: 'Caisse Desjardins', kind: 'savings' })).toBe('desjardins');
    expect(suggestParser({ institutionName: 'Desjardins', kind: 'credit_card' })).toBe('desjardins-credit');
    expect(suggestParser({ institutionName: 'Wealthsimple', kind: 'checking' })).toBe('wealthsimple');
    expect(suggestParser({ institutionName: 'WEALTHSIMPLE', kind: 'credit_card' })).toBe('wealthsimple-credit');
  });
  it('returns empty for unknown banks', () => {
    expect(suggestParser({ institutionName: 'Tangerine', kind: 'checking' })).toBe('');
    expect(suggestParser(undefined)).toBe('');
  });
});

describe('extensionOf', () => {
  it('lowercases and handles odd names', () => {
    expect(extensionOf('/a/b/Releve.OFX')).toBe('ofx');
    expect(extensionOf('noext')).toBe('');
    expect(extensionOf('a.b.csv')).toBe('csv');
  });
});
