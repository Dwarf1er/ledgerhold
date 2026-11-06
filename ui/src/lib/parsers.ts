import type { Account } from './api';

/** Best-guess parser for an account (the user can always override). */
export function suggestParser(a: Pick<Account, 'institutionName' | 'kind'> | undefined): string {
  if (!a) return '';
  const bank = a.institutionName.toLowerCase();
  const card = a.kind === 'credit_card';
  if (bank.includes('desjardins')) return card ? 'desjardins-credit' : 'desjardins';
  if (bank.includes('wealthsimple')) return card ? 'wealthsimple-credit' : 'wealthsimple';
  return '';
}

export function extensionOf(path: string): string {
  const m = /\.([A-Za-z0-9]+)$/.exec(path);
  return m ? m[1].toLowerCase() : '';
}

/** Formats we can recognise without being told which bank it is. */
export const SELF_DESCRIBING = ['ofx', 'qfx', 'qbo', 'pdf'];
