// Typed bridge to the Rust service layer (crates/app). Types mirror crates/app/src/dto.rs.
// Money is integer minor units (cents); dates are ISO `YYYY-MM-DD`.

export interface Person {
  id: string;
  name: string;
}
export interface Institution {
  id: string;
  name: string;
  bankId: string | null;
}
export type AccountKind = 'checking' | 'savings' | 'credit_card';
export interface Account {
  id: string;
  name: string;
  institutionId: string;
  institutionName: string;
  kind: AccountKind;
  currency: string;
  number: string | null;
  ownerIds: string[];
}
export interface AccountSummary {
  account: Account;
  balance: number | null;
  txnCount: number;
  firstDate: string | null;
  lastDate: string | null;
}
export interface Overview {
  people: Person[];
  institutions: Institution[];
  accounts: AccountSummary[];
}
export interface Anchor {
  id: string;
  date: string;
  amount: number;
  note: string | null;
}
export interface Batch {
  id: string;
  parser: string;
  filename: string;
  importedAt: string;
  periodStart: string | null;
  periodEnd: string | null;
  rowsNew: number;
  rowsDuplicate: number;
  rowsError: number;
}
export interface Discrepancy {
  from: string;
  fromKind: 'anchor' | 'statement';
  to: string;
  toKind: 'anchor' | 'statement';
  expected: number;
  actual: number;
  difference: number;
  ok: boolean;
}
export interface AccountDetail {
  summary: AccountSummary;
  anchors: Anchor[];
  batches: Batch[];
  reconciliation: Discrepancy[];
}
export interface Txn {
  id: string;
  date: string;
  postedDate: string | null;
  amount: number;
  description: string;
  balance: number | null;
  batchId: string;
}
export interface BalancePoint {
  date: string;
  balance: number;
}
export interface PreviewRow {
  date: string;
  amount: number;
  description: string;
  status: 'new' | 'duplicate';
}
export interface RowError {
  rowRef: string;
  message: string;
}
export interface ImportPreview {
  fileName: string;
  parser: string;
  accountHint: string | null;
  currency: string;
  period: [string, string] | null;
  rows: PreviewRow[];
  newCount: number;
  duplicateCount: number;
  errors: RowError[];
  warnings: string[];
  fileAlreadyImported: boolean;
}
export interface ImportResult {
  batchId: string | null;
  newCount: number;
  duplicateCount: number;
  errorCount: number;
}
export interface ExportResult {
  path: string;
  accountName: string;
  start: string;
  end: string;
  txnCount: number;
  balance: number | null;
}
export interface Parser {
  id: string;
  description: string;
}
export interface NewAccountInput {
  institutionId: string;
  name: string;
  kind: AccountKind;
  currency: string;
  number: string | null;
  ownerIds: string[];
}

export interface Api {
  overview(): Promise<Overview>;
  accountDetail(accountId: string): Promise<AccountDetail>;
  transactions(accountId: string, from?: string | null, to?: string | null, search?: string | null): Promise<Txn[]>;
  balanceSeries(accountId: string, start: string, end: string): Promise<BalancePoint[]>;
  parsers(): Promise<Parser[]>;
  previewImport(accountId: string, path: string, parser?: string | null): Promise<ImportPreview>;
  commitImport(accountId: string, path: string, parser?: string | null): Promise<ImportResult>;
  undoBatch(batchId: string): Promise<number>;
  setAnchor(accountId: string, date: string, amount: number, note?: string | null): Promise<void>;
  deleteAnchor(anchorId: string): Promise<void>;
  exportOfx(accountIds: string[], start: string, end: string, dir: string): Promise<ExportResult[]>;
  addPerson(name: string): Promise<Person>;
  addInstitution(name: string, bankId?: string | null): Promise<Institution>;
  addAccount(input: NewAccountInput): Promise<string>;
  setOwners(accountId: string, ownerIds: string[]): Promise<void>;
  dbPath(): Promise<string>;
  /** Native file picker; resolves to a path or null if cancelled. */
  pickFile(): Promise<string | null>;
  pickFolder(): Promise<string | null>;
}

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const tauriApi: Api = {
  async overview() {
    return call('overview');
  },
  async accountDetail(accountId) {
    return call('account_detail', { accountId });
  },
  async transactions(accountId, from = null, to = null, search = null) {
    return call('transactions', { accountId, from, to, search });
  },
  async balanceSeries(accountId, start, end) {
    return call('balance_series', { accountId, start, end });
  },
  async parsers() {
    return call('parsers');
  },
  async previewImport(accountId, path, parser = null) {
    return call('preview_import', { accountId, path, parser });
  },
  async commitImport(accountId, path, parser = null) {
    return call('commit_import', { accountId, path, parser });
  },
  async undoBatch(batchId) {
    return call('undo_batch', { batchId });
  },
  async setAnchor(accountId, date, amount, note = null) {
    return call('set_anchor', { accountId, date, amount, note });
  },
  async deleteAnchor(anchorId) {
    return call('delete_anchor', { anchorId });
  },
  async exportOfx(accountIds, start, end, dir) {
    return call('export_ofx', { accountIds, start, end, dir });
  },
  async addPerson(name) {
    return call('add_person', { name });
  },
  async addInstitution(name, bankId = null) {
    return call('add_institution', { name, bankId });
  },
  async addAccount(input) {
    return call('add_account', { input });
  },
  async setOwners(accountId, ownerIds) {
    return call('set_owners', { accountId, ownerIds });
  },
  async dbPath() {
    return call('db_path');
  },
  async pickFile() {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const r = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Statements', extensions: ['csv', 'pdf', 'ofx', 'qfx', 'qbo'] }],
    });
    return typeof r === 'string' ? r : null;
  },
  async pickFolder() {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const r = await open({ multiple: false, directory: true });
    return typeof r === 'string' ? r : null;
  },
};

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    // Commands reject with a plain message string.
    throw new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));
  }
}

let impl: Promise<Api> | null = null;
function resolve(): Promise<Api> {
  return (impl ??= isTauri ? Promise.resolve(tauriApi) : import('./mock').then((m) => m.createMockApi()));
}

/** Outside Tauri (plain browser, tests) calls go to an in-memory mock. */
export const api: Api = new Proxy({} as Api, {
  get:
    (_t, key: string) =>
    (...args: unknown[]) =>
      resolve().then((i) => (i as unknown as Record<string, (...a: unknown[]) => unknown>)[key](...args)),
});

/** Test hook: replace the backend. */
export function __setApi(a: Api | null) {
  impl = a ? Promise.resolve(a) : null;
}
