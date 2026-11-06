import { describe, expect, it } from 'vitest';
import {
  formatMoney,
  formatSigned,
  initials,
  lastFullMonth,
  maskNumber,
  monthRange,
  parseAmount,
  shiftMonth,
  toISO,
  parseISO,
} from './format';

describe('parseAmount', () => {
  it.each([
    ['1234.56', 123456],
    ['1,234.56', 123456],
    ['1 234,56', 123456],
    ['1 234,56', 123456],
    ['$12', 1200],
    ['-12.5', -1250],
    ['(12.34)', -1234],
    ['+0.05', 5],
    ['.99', 99],
    ['1,000', 100000],
    ['12,34', 1234],
  ])('%s -> %d', (input, cents) => {
    expect(parseAmount(input)).toBe(cents);
  });

  it.each([[''], ['abc'], ['$'], ['-'], ['1.2.x']])('rejects %j', (input) => {
    expect(parseAmount(input)).toBeNull();
  });
});

describe('money formatting', () => {
  it('formats cents with the currency', () => {
    expect(formatMoney(123456, 'CAD')).toBe('$1,234.56');
    expect(formatMoney(-5, 'CAD')).toBe('-$0.05');
    expect(formatMoney(100, 'USD')).toContain('1.00');
  });
  it('signs deltas', () => {
    expect(formatSigned(250)).toBe('+$2.50');
    expect(formatSigned(-250)).toBe('−$2.50');
    expect(formatSigned(0)).toBe('$0.00');
  });
});

describe('dates', () => {
  it('month ranges handle leap years and december', () => {
    expect(monthRange(2024, 2)).toEqual({ start: '2024-02-01', end: '2024-02-29' });
    expect(monthRange(2026, 12)).toEqual({ start: '2026-12-01', end: '2026-12-31' });
  });
  it('last full month wraps the year', () => {
    expect(lastFullMonth(new Date(2026, 0, 15))).toEqual({ year: 2025, month: 12 });
    expect(lastFullMonth(new Date(2026, 10, 6))).toEqual({ year: 2026, month: 10 });
  });
  it('shiftMonth moves across years both ways', () => {
    expect(shiftMonth(2026, 1, -1)).toEqual({ year: 2025, month: 12 });
    expect(shiftMonth(2026, 12, 1)).toEqual({ year: 2027, month: 1 });
  });
  it('ISO roundtrip has no timezone drift', () => {
    expect(toISO(parseISO('2026-03-08'))).toBe('2026-03-08');
    expect(toISO(parseISO('2026-11-01'))).toBe('2026-11-01');
  });
});

describe('labels', () => {
  it('initials', () => {
    expect(initials('Alex')).toBe('A');
    expect(initials('Sam Lee')).toBe('SL');
    expect(initials('  ')).toBe('?');
  });
  it('maskNumber shows only the tail', () => {
    expect(maskNumber('EOP-12345')).toBe('•••• 2345');
    expect(maskNumber(null)).toBe('');
  });
});
