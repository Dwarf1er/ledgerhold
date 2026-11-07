// In-memory stand-in for the Rust backend, used when the UI runs in a plain
// browser (dev without Tauri) and in tests. Mirrors crates/app semantics:
// anchors are end-of-day balances; balances work forward/backward from them.
import type {
  Account,
  AccountDetail,
  AccountKind,
  AccountSummary,
  Anchor,
  Api,
  BalancePoint,
  Batch,
  ExportResult,
  ImportPreview,
  ImportResult,
  Institution,
  NewAccountInput,
  Overview,
  Person,
  Txn,
} from './api';
import { parseISO, toISO } from './format';

interface MockTxn {
  id: string;
  date: string;
  amount: number;
  description: string;
  batchId: string;
}

function rng(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

const addDays = (iso: string, n: number) => {
  const d = parseISO(iso);
  d.setDate(d.getDate() + n);
  return toISO(d);
};

export function createMockApi(opts: { today?: string } = {}): Api {
  const today = opts.today ?? toISO(new Date());
  let seq = 0;
  const id = (p: string) => `${p}-${++seq}`;
  const rand = rng(42);
  const between = (lo: number, hi: number) => Math.round(lo + rand() * (hi - lo));

  const people: Person[] = [
    { id: 'p-alex', name: 'Alex' },
    { id: 'p-sam', name: 'Sam' },
  ];
  const institutions: Institution[] = [
    { id: 'i-desj', name: 'Desjardins', bankId: '815' },
    { id: 'i-ws', name: 'Wealthsimple', bankId: null },
  ];
  const accounts: Account[] = [
    { id: 'a-joint', name: 'Joint chequing', institutionId: 'i-desj', institutionName: 'Desjardins', kind: 'checking', currency: 'CAD', number: 'EOP-12345', ownerIds: ['p-alex', 'p-sam'] },
    { id: 'a-save', name: 'Alex savings', institutionId: 'i-desj', institutionName: 'Desjardins', kind: 'savings', currency: 'CAD', number: 'PR1-67890', ownerIds: ['p-alex'] },
    { id: 'a-ws', name: 'Sam everyday', institutionId: 'i-ws', institutionName: 'Wealthsimple', kind: 'checking', currency: 'CAD', number: null, ownerIds: ['p-sam'] },
    { id: 'a-visa', name: 'Visa Desjardins', institutionId: 'i-desj', institutionName: 'Desjardins', kind: 'credit_card', currency: 'CAD', number: '4540XXXXXXXX1234', ownerIds: ['p-alex', 'p-sam'] },
    { id: 'a-usd', name: 'Travel card (USD)', institutionId: 'i-ws', institutionName: 'Wealthsimple', kind: 'credit_card', currency: 'USD', number: null, ownerIds: ['p-sam'] },
  ];
  const txns = new Map<string, MockTxn[]>(accounts.map((a) => [a.id, []]));
  const anchors = new Map<string, Anchor[]>(accounts.map((a) => [a.id, []]));
  const batches = new Map<string, Batch[]>(accounts.map((a) => [a.id, []]));

  const start = addDays(today, -150);
  const push = (acct: string, date: string, amount: number, description: string, batch = 'b-seed') => {
    if (date >= start && date <= today) txns.get(acct)!.push({ id: id('t'), date, amount, description, batchId: batch });
  };

  // Seed history.
  const groceries = ['IGA EXTRA', 'MAXI', 'METRO PLUS', 'MARCHE JEAN-TALON'];
  const coffee = ['CAFE MYRIADE', 'SECOND CUP', 'STARBUCKS'];
  const shops = ['AMZN MKTP CA', 'UBER *TRIP', 'SAQ', 'PHARMAPRIX', 'STM OPUS', 'RESTAURANT LE GOURMET', 'CINEMA BEAUBIEN'];
  for (let n = 0; n <= 150; n++) {
    const date = addDays(start, n);
    const dt = parseISO(date);
    const dom = dt.getDate();
    const dow = dt.getDay();
    if (dow === 5 && Math.floor(n / 7) % 2 === 0) push('a-joint', date, 245000, 'PAIE ACME INC');
    if (dom === 1) {
      push('a-joint', date, -142500, 'LOYER - GESTION IMMO');
      push('a-joint', date, 198000, 'PAIE STUDIO NORD');
      push('a-joint', date, -50000, 'VIREMENT VERS EPARGNE');
      push('a-save', date, 50000, 'VIREMENT DE CHEQUES');
    }
    if (dom === 5) push('a-joint', date, -6990, 'VIDEOTRON');
    if (dom === 12) push('a-joint', date, -between(7800, 12000), 'HYDRO-QUEBEC');
    if (dom === 20) {
      push('a-joint', date, -55000, 'PAIEMENT VISA');
      push('a-visa', date, 55000, 'PAIEMENT CAISSE');
    }
    if (dow === 6) push('a-joint', date, -between(8000, 19000), groceries[between(0, 3)]);
    if (rand() < 0.35) push('a-joint', date, -between(450, 780), coffee[between(0, 2)]);
    if (dom === 28) push('a-save', date, between(600, 1100), 'INTERETS');
    if (rand() < 0.3) push('a-visa', date, -between(1500, 14000), shops[between(0, 6)]);
    if (dom === 8) push('a-visa', date, -1649, 'NETFLIX.COM');
    if (dom === 15) push('a-ws', date, 80000, 'PAYMENT FROM SAM');
    if (rand() < 0.15) push('a-ws', date, -between(900, 5500), shops[between(0, 6)]);
  }
  push('a-usd', addDays(today, -40), -12850, 'HOTEL LISBOA');
  push('a-usd', addDays(today, -39), -4210, 'TAXI AIRPORT');
  push('a-usd', addDays(today, -37), -2290, 'CAFE DE LISBOA');

  const byDate = (a: MockTxn, b: MockTxn) => (a.date < b.date ? -1 : a.date > b.date ? 1 : 0);
  for (const list of txns.values()) list.sort(byDate);

  const sumBetween = (acct: string, afterExcl: string, upto: string) =>
    txns.get(acct)!.filter((t) => t.date > afterExcl && t.date <= upto).reduce((s, t) => s + t.amount, 0);

  function balanceAt(acct: string, on: string): number | null {
    const as = [...anchors.get(acct)!].sort((a, b) => (a.date < b.date ? -1 : 1));
    const before = [...as].reverse().find((a) => a.date <= on);
    if (before) return before.amount + sumBetween(acct, before.date, on);
    const after = as.find((a) => a.date > on);
    if (after) return after.amount - sumBetween(acct, on, after.date);
    return null;
  }

  const openings: Record<string, number> = { 'a-joint': 425000, 'a-save': 1250000, 'a-ws': 86000, 'a-visa': -82000 };
  for (const [acct, amount] of Object.entries(openings)) {
    anchors.get(acct)!.push({ id: id('an'), date: addDays(start, -1), amount, note: 'Opening balance' });
  }
  // A matching statement balance, then one that is off by 42.18 (demonstrates reconciliation).
  const ok = addDays(today, -60);
  anchors.get('a-joint')!.push({ id: id('an'), date: ok, amount: balanceAt('a-joint', ok)!, note: 'Statement' });
  const bad = addDays(today, -14);
  anchors.get('a-joint')!.push({ id: id('an'), date: bad, amount: balanceAt('a-joint', bad)! - 4218, note: 'Statement' });

  batches.get('a-joint')!.push({
    id: 'b-seed', parser: 'desjardins', filename: 'releve-desjardins.csv', importedAt: `${addDays(today, -30)}T14:02:11Z`,
    periodStart: start, periodEnd: addDays(today, -30), rowsNew: txns.get('a-joint')!.length, rowsDuplicate: 0, rowsError: 2,
  });

  const summary = (a: Account): AccountSummary => {
    const list = txns.get(a.id)!;
    return {
      account: a,
      balance: balanceAt(a.id, today),
      txnCount: list.length,
      firstDate: list[0]?.date ?? null,
      lastDate: list[list.length - 1]?.date ?? null,
    };
  };
  const find = (accountId: string) => {
    const a = accounts.find((x) => x.id === accountId);
    if (!a) throw new Error(`account ${accountId} not found`);
    return a;
  };
  const delay = <T>(v: T) => new Promise<T>((r) => setTimeout(() => r(v), 40));

  const api: Api = {
    overview: async () =>
      delay<Overview>({ people: [...people], institutions: [...institutions], accounts: accounts.map(summary) }),

    accountDetail: async (accountId) => {
      const a = find(accountId);
      const as = [...anchors.get(a.id)!].sort((x, y) => (x.date < y.date ? -1 : 1));
      const reconciliation = as.slice(1).map((to, i) => {
        const from = as[i];
        const expected = from.amount + sumBetween(a.id, from.date, to.date);
        return { from: from.date, fromKind: 'anchor' as const, to: to.date, toKind: 'anchor' as const, expected, actual: to.amount, difference: to.amount - expected, ok: to.amount === expected };
      });
      return delay<AccountDetail>({ summary: summary(a), anchors: as, batches: [...batches.get(a.id)!], reconciliation });
    },

    transactions: async (accountId, from = null, to = null, search = null) => {
      const list = txns.get(find(accountId).id)!;
      const needle = (search ?? '').trim().toUpperCase();
      let running = list.length ? balanceAt(accountId, addDays(list[0].date, -1)) : null;
      const out: Txn[] = [];
      for (const t of list) {
        if (running !== null) running += t.amount;
        const inRange = (!from || t.date >= from) && (!to || t.date <= to);
        const hit = !needle || t.description.toUpperCase().includes(needle) || (t.amount / 100).toFixed(2).includes(needle);
        if (inRange && hit) out.push({ id: t.id, date: t.date, postedDate: null, amount: t.amount, description: t.description, balance: running, batchId: t.batchId });
      }
      return delay(out.reverse());
    },

    balanceSeries: async (accountId, startDate, endDate) => {
      const out: BalancePoint[] = [];
      for (let d = startDate; d <= endDate; d = addDays(d, 1)) {
        const b = balanceAt(accountId, d);
        if (b === null) throw new Error('account has no balance anchor; set a starting balance first');
        out.push({ date: d, balance: b });
      }
      return delay(out);
    },

    parsers: async () => [
      { id: 'desjardins', description: 'Desjardins chequing/savings CSV' },
      { id: 'desjardins-credit', description: 'Desjardins Visa PDF statement' },
      { id: 'ofx', description: 'OFX/QFX/QBO statement (any bank)' },
      { id: 'wealthsimple', description: 'Wealthsimple chequing CSV' },
      { id: 'wealthsimple-credit', description: 'Wealthsimple credit card CSV' },
    ],

    previewImport: async (accountId, path, parser = null) => {
      const a = find(accountId);
      if (!parser && !/\.(ofx|qfx|qbo|pdf)$/i.test(path)) throw new Error("can't tell the file format; choose a parser");
      const existing = txns.get(a.id)!.slice(-4);
      const fresh = [
        [addDays(today, -3), -4575, 'IGA EXTRA'],
        [addDays(today, -2), -612, 'CAFE MYRIADE'],
        [addDays(today, -2), -2399, 'UBER *TRIP'],
        [addDays(today, -1), 245000, 'PAIE ACME INC'],
        [addDays(today, -1), -3200, 'PHARMAPRIX'],
      ] as const;
      const rows = [
        ...existing.map((t) => ({ date: t.date, amount: t.amount, description: t.description, status: 'duplicate' as const })),
        ...fresh.map(([date, amount, description]) => ({ date, amount, description, status: 'new' as const })),
      ];
      const name = path.split('/').pop() ?? path;
      const p: ImportPreview = {
        fileName: name,
        parser: parser ?? (/\.pdf$/i.test(path) ? 'desjardins-credit' : 'ofx'),
        accountHint: a.number,
        currency: a.currency,
        period: [rows[0].date, rows[rows.length - 1].date],
        rows,
        newCount: fresh.length,
        duplicateCount: existing.length,
        errors: [{ rowRef: 'line 14', message: 'no amount in columns 6-12 (informational row?)' }],
        warnings: [],
        fileAlreadyImported: false,
      };
      return delay(p);
    },

    commitImport: async (accountId, path, parser = null) => {
      const preview = await api.previewImport(accountId, path, parser);
      const batchId = id('b');
      for (const r of preview.rows.filter((x) => x.status === 'new')) {
        txns.get(accountId)!.push({ id: id('t'), date: r.date, amount: r.amount, description: r.description, batchId });
      }
      txns.get(accountId)!.sort(byDate);
      batches.get(accountId)!.push({
        id: batchId, parser: preview.parser, filename: preview.fileName, importedAt: `${today}T09:00:00Z`,
        periodStart: preview.period?.[0] ?? null, periodEnd: preview.period?.[1] ?? null,
        rowsNew: preview.newCount, rowsDuplicate: preview.duplicateCount, rowsError: preview.errors.length,
      });
      const r: ImportResult = { batchId, newCount: preview.newCount, duplicateCount: preview.duplicateCount, errorCount: preview.errors.length };
      return delay(r);
    },

    undoBatch: async (batchId) => {
      let n = 0;
      for (const [acct, list] of txns) {
        const keep = list.filter((t) => t.batchId !== batchId);
        n += list.length - keep.length;
        txns.set(acct, keep);
        batches.set(acct, batches.get(acct)!.filter((b) => b.id !== batchId));
      }
      return delay(n);
    },

    setAnchor: async (accountId, date, amount, note = null) => {
      const list = anchors.get(find(accountId).id)!;
      const i = list.findIndex((a) => a.date === date);
      const a: Anchor = { id: i >= 0 ? list[i].id : id('an'), date, amount, note: note || null };
      if (i >= 0) list[i] = a;
      else list.push(a);
      return delay(undefined);
    },
    deleteAnchor: async (anchorId) => {
      for (const [acct, list] of anchors) anchors.set(acct, list.filter((a) => a.id !== anchorId));
      return delay(undefined);
    },

    exportOfx: async (accountIds, startDate, endDate, dir) => {
      if (endDate < startDate) throw new Error('end date is before start date');
      const out: ExportResult[] = accountIds.map((aid) => {
        const a = find(aid);
        const count = txns.get(aid)!.filter((t) => t.date >= startDate && t.date <= endDate).length;
        return {
          path: `${dir}/${a.name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${startDate.replaceAll('-', '')}-${endDate.replaceAll('-', '')}.ofx`,
          accountName: a.name, start: startDate, end: endDate, txnCount: count, balance: balanceAt(aid, endDate),
        };
      });
      return delay(out);
    },

    addPerson: async (name) => {
      if (!name.trim()) throw new Error('name is required');
      const p = { id: id('p'), name: name.trim() };
      people.push(p);
      return delay(p);
    },
    renamePerson: async (pid, name) => {
      if (!name.trim()) throw new Error('name is required');
      const p = people.find((x) => x.id === pid);
      if (!p) throw new Error(`person ${pid} not found`);
      p.name = name.trim();
      return delay({ ...p });
    },
    deletePerson: async (pid) => {
      const i = people.findIndex((x) => x.id === pid);
      if (i < 0) throw new Error(`person ${pid} not found`);
      people.splice(i, 1);
      for (const a of accounts) a.ownerIds = a.ownerIds.filter((o) => o !== pid);
      return delay(undefined);
    },
    renameAccount: async (aid, name) => {
      if (!name.trim()) throw new Error('name is required');
      find(aid).name = name.trim();
      return delay(undefined);
    },
    deleteAccount: async (aid) => {
      find(aid);
      const n = txns.get(aid)!.length;
      accounts.splice(accounts.findIndex((x) => x.id === aid), 1);
      txns.delete(aid);
      anchors.delete(aid);
      batches.delete(aid);
      return delay(n);
    },
    addInstitution: async (name, bankId = null) => {
      if (!name.trim()) throw new Error('name is required');
      const i = { id: id('i'), name: name.trim(), bankId: bankId?.trim() || null };
      institutions.push(i);
      return delay(i);
    },
    updateInstitution: async (iid, name, bankId = null) => {
      if (!name.trim()) throw new Error('name is required');
      const inst = institutions.find((i) => i.id === iid);
      if (!inst) throw new Error(`institution ${iid} not found`);
      inst.name = name.trim();
      inst.bankId = bankId?.trim() || null;
      for (const a of accounts) if (a.institutionId === iid) a.institutionName = inst.name;
      return delay({ ...inst });
    },
    deleteInstitution: async (iid) => {
      const i = institutions.findIndex((x) => x.id === iid);
      if (i < 0) throw new Error(`institution ${iid} not found`);
      const n = accounts.filter((a) => a.institutionId === iid).length;
      if (n > 0) throw new Error(`this bank still has ${n} account${n === 1 ? '' : 's'}; delete ${n === 1 ? 'it' : 'them'} first`);
      institutions.splice(i, 1);
      return delay(undefined);
    },
    addAccount: async (input: NewAccountInput) => {
      const inst = institutions.find((i) => i.id === input.institutionId);
      if (!inst) throw new Error('institution not found');
      if (!input.name.trim()) throw new Error('name is required');
      const a: Account = {
        id: id('a'), name: input.name.trim(), institutionId: inst.id, institutionName: inst.name,
        kind: input.kind as AccountKind, currency: input.currency.trim().toUpperCase(),
        number: input.number?.trim() || null, ownerIds: [...input.ownerIds],
      };
      accounts.push(a);
      txns.set(a.id, []);
      anchors.set(a.id, []);
      batches.set(a.id, []);
      return delay(a.id);
    },
    setOwners: async (accountId, ownerIds) => {
      find(accountId).ownerIds = [...ownerIds];
      return delay(undefined);
    },
    dbPath: async () => '~/.local/share/ledgerhold/ledgerhold.sqlite (demo data)',
    pickFile: async () => '/home/demo/Downloads/releve-janvier.ofx',
    pickFolder: async () => '/home/demo/Documents/budget',
    uploadFile: async (file) => `/home/demo/Downloads/${file.name}`,
    session: async () => ({ authRequired: false, authenticated: true, setupRequired: false, canChangePassword: false }),
    login: async () => {},
    logout: async () => {},
    setup: async () => {},
    changePassword: async () => {},
  };

  return api;
}
