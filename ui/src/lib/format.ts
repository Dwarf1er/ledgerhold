const locale = 'en-CA';

const formatters = new Map<string, Intl.NumberFormat>();
function nf(currency: string): Intl.NumberFormat {
  let f = formatters.get(currency);
  if (!f) {
    f = new Intl.NumberFormat(locale, { style: 'currency', currency });
    formatters.set(currency, f);
  }
  return f;
}

/** `minor` is integer cents. */
export function formatMoney(minor: number, currency = 'CAD'): string {
  return nf(currency).format(minor / 100);
}

/** Like formatMoney but always shows a sign (+/−), for deltas and differences. */
export function formatSigned(minor: number, currency = 'CAD'): string {
  const s = formatMoney(Math.abs(minor), currency);
  return minor > 0 ? `+${s}` : minor < 0 ? `−${s}` : s;
}

/**
 * Parse what a person types into cents. Accepts `1234.56`, `1,234.56`,
 * `1 234,56`, `$12`, `-12.5`, `(12.34)`; returns null when it isn't an amount.
 */
export function parseAmount(input: string): number | null {
  let s = input.trim().replace(/[\s  $€£]/g, '');
  if (!s) return null;
  let negative = false;
  if (s.startsWith('(') && s.endsWith(')')) {
    negative = true;
    s = s.slice(1, -1);
  }
  if (s.startsWith('-') || s.startsWith('−')) {
    negative = true;
    s = s.slice(1);
  } else if (s.startsWith('+')) {
    s = s.slice(1);
  }
  if (!/^[\d.,]+$/.test(s) || !/\d/.test(s)) return null;
  const last = Math.max(s.lastIndexOf('.'), s.lastIndexOf(','));
  let int = s;
  let frac = '';
  if (last >= 0) {
    const sep = s[last];
    const after = s.slice(last + 1);
    const sameCount = s.split(sep).length - 1;
    // A lone separator followed by 1-2 digits is the decimal mark; otherwise thousands.
    if (sameCount === 1 && after.length <= 2) {
      int = s.slice(0, last);
      frac = after;
    }
  }
  int = int.replace(/[.,]/g, '');
  if (int === '' && frac === '') return null;
  const cents = Number(int || '0') * 100 + Number((frac + '00').slice(0, 2));
  if (!Number.isSafeInteger(cents)) return null;
  return negative ? -cents : cents;
}

export function parseISO(iso: string): Date {
  const [y, m, d] = iso.split('-').map(Number);
  return new Date(y, m - 1, d);
}

export function toISO(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

const dateFmt = new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: 'numeric' });
const shortFmt = new Intl.DateTimeFormat(locale, { month: 'short', day: 'numeric' });
const monthFmt = new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'long' });

export const formatDate = (iso: string) => dateFmt.format(parseISO(iso));
export const formatShortDate = (iso: string) => shortFmt.format(parseISO(iso));
export const formatMonth = (y: number, m: number) => monthFmt.format(new Date(y, m - 1, 1));

export function monthRange(year: number, month: number): { start: string; end: string } {
  return { start: toISO(new Date(year, month - 1, 1)), end: toISO(new Date(year, month, 0)) };
}

/** Previous calendar month relative to `today`. */
export function lastFullMonth(today: Date): { year: number; month: number } {
  const d = new Date(today.getFullYear(), today.getMonth() - 1, 1);
  return { year: d.getFullYear(), month: d.getMonth() + 1 };
}

export function shiftMonth(year: number, month: number, delta: number): { year: number; month: number } {
  const d = new Date(year, month - 1 + delta, 1);
  return { year: d.getFullYear(), month: d.getMonth() + 1 };
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  return ((parts[0]?.[0] ?? '') + (parts.length > 1 ? parts[parts.length - 1][0] : '')).toUpperCase() || '?';
}

export const kindLabel = (kind: string) =>
  kind === 'credit_card' ? 'Credit card' : kind === 'savings' ? 'Savings' : 'Chequing';

export function maskNumber(n: string | null | undefined): string {
  if (!n) return '';
  const tail = n.replace(/[^A-Za-z0-9]/g, '').slice(-4);
  return `•••• ${tail}`;
}
