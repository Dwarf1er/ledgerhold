export type Route =
  | { name: 'overview' }
  | { name: 'account'; id: string; tab: 'transactions' | 'balance' | 'imports' }
  | { name: 'import'; accountId: string | null }
  | { name: 'settings' };

export function parseHash(hash: string): Route {
  const raw = hash.replace(/^#/, '') || '/';
  const [path, query = ''] = raw.split('?');
  const q = new URLSearchParams(query);
  const parts = path.split('/').filter(Boolean);
  if (parts[0] === 'account' && parts[1]) {
    const tab = q.get('tab');
    return {
      name: 'account',
      id: decodeURIComponent(parts[1]),
      tab: tab === 'balance' || tab === 'imports' ? tab : 'transactions',
    };
  }
  if (parts[0] === 'import') return { name: 'import', accountId: q.get('account') };
  if (parts[0] === 'settings') return { name: 'settings' };
  return { name: 'overview' };
}

export const router = $state<{ route: Route }>({ route: parseHash(typeof location === 'undefined' ? '' : location.hash) });

if (typeof window !== 'undefined') {
  window.addEventListener('hashchange', () => {
    router.route = parseHash(location.hash);
  });
}

export const links = {
  overview: () => '#/',
  account: (id: string, tab?: 'transactions' | 'balance' | 'imports') =>
    `#/account/${encodeURIComponent(id)}${tab && tab !== 'transactions' ? `?tab=${tab}` : ''}`,
  import: (accountId?: string) => `#/import${accountId ? `?account=${encodeURIComponent(accountId)}` : ''}`,
  settings: () => '#/settings',
};

export function navigate(href: string): void {
  location.hash = href;
}
