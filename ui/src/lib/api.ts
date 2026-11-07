// Typed bridge to the Rust service layer (crates/app). Types mirror crates/app/src/dto.rs.
// Money is integer minor units (cents); dates are ISO `YYYY-MM-DD`.

export interface Person {
  id: string;
  name: string;
  /** Avatar colour name (see lib/colors.ts). */
  color: string;
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
  index: number;
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
  commitImport(accountId: string, path: string, parser?: string | null, excluded?: number[]): Promise<ImportResult>;
  undoBatch(batchId: string): Promise<number>;
  setAnchor(accountId: string, date: string, amount: number, note?: string | null): Promise<void>;
  deleteAnchor(anchorId: string): Promise<void>;
  exportOfx(accountIds: string[], start: string, end: string, dir: string): Promise<ExportResult[]>;
  /** Without a colour, the person gets the first one nobody has. */
  addPerson(name: string, color?: string | null): Promise<Person>;
  /** Changes the name, and the colour when one is given. */
  updatePerson(id: string, name: string, color?: string | null): Promise<Person>;
  /** Accounts they owned stay, without them as an owner. */
  deletePerson(id: string): Promise<void>;
  addInstitution(name: string, bankId?: string | null): Promise<Institution>;
  updateInstitution(id: string, name: string, bankId?: string | null): Promise<Institution>;
  /** Refused while the bank still has accounts. */
  deleteInstitution(id: string): Promise<void>;
  addAccount(input: NewAccountInput): Promise<string>;
  renameAccount(id: string, name: string): Promise<void>;
  updateAccount(id: string, name: string, number: string | null): Promise<void>;
  /** Removes the account with its transactions, imports and balances; resolves to the transactions removed. */
  deleteAccount(id: string): Promise<number>;
  setOwners(accountId: string, ownerIds: string[]): Promise<void>;
  dbPath(): Promise<string>;
  /** Native file picker; resolves to a path or null if cancelled. In a browser this uploads the file. */
  pickFile(): Promise<string | null>;
  pickFolder(): Promise<string | null>;
  /** Browser only: upload a dropped file and return the opaque path to import it from. */
  uploadFile(file: File): Promise<string>;
  session(): Promise<SessionInfo>;
  login(password: string): Promise<void>;
  logout(): Promise<void>;
  /** First run of a self-hosted server: choose the household password with the code from the server log. */
  setup(code: string, password: string): Promise<void>;
  changePassword(current: string, newPassword: string): Promise<void>;
}

export interface SessionInfo {
  authRequired: boolean;
  authenticated: boolean;
  /** The server has no password yet and is waiting for first-run setup. */
  setupRequired: boolean;
  /** The password is stored by the app (not set through the environment), so it can be changed here. */
  canChangePassword: boolean;
}

/** Which backend serves this page: the desktop app, the self-hosted server, or in-browser demo data. */
export type Backend = 'tauri' | 'http' | 'mock';

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const backend: Backend = isTauri ? 'tauri' : import.meta.env.DEV ? 'mock' : 'http';

/**
 * What this backend can do. The desktop app and the demo write exports to a chosen folder;
 * a browser downloads them instead. (An object, not a constant, so tests can flip it.)
 */
export const capabilities = { pickFolder: backend !== 'http' };

export type Call = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/**
 * Every backend command, written once. The desktop app invokes them over Tauri IPC and the
 * server over HTTP; contract.test.ts checks these names against both Rust sides.
 */
export function backed(call: Call) {
  return {
    overview: () => call<Overview>('overview'),
    accountDetail: (accountId: string) => call<AccountDetail>('account_detail', { accountId }),
    transactions: (accountId: string, from: string | null = null, to: string | null = null, search: string | null = null) =>
      call<Txn[]>('transactions', { accountId, from, to, search }),
    balanceSeries: (accountId: string, start: string, end: string) =>
      call<BalancePoint[]>('balance_series', { accountId, start, end }),
    parsers: () => call<Parser[]>('parsers'),
    previewImport: (accountId: string, path: string, parser: string | null = null) =>
      call<ImportPreview>('preview_import', { accountId, path, parser }),
    commitImport: (accountId: string, path: string, parser: string | null = null, excluded: number[] = []) =>
      call<ImportResult>('commit_import', { accountId, path, parser, excluded }),
    undoBatch: (batchId: string) => call<number>('undo_batch', { batchId }),
    setAnchor: (accountId: string, date: string, amount: number, note: string | null = null) =>
      call<void>('set_anchor', { accountId, date, amount, note }),
    deleteAnchor: (anchorId: string) => call<void>('delete_anchor', { anchorId }),
    addPerson: (name: string, color: string | null = null) => call<Person>('add_person', { name, color }),
    updatePerson: (id: string, name: string, color: string | null = null) =>
      call<Person>('update_person', { id, name, color }),
    deletePerson: (id: string) => call<void>('delete_person', { id }),
    addInstitution: (name: string, bankId: string | null = null) =>
      call<Institution>('add_institution', { name, bankId }),
    updateInstitution: (id: string, name: string, bankId: string | null = null) =>
      call<Institution>('update_institution', { id, name, bankId }),
    deleteInstitution: (id: string) => call<void>('delete_institution', { id }),
    addAccount: (input: NewAccountInput) => call<string>('add_account', { input }),
    renameAccount: (id: string, name: string) => call<void>('rename_account', { id, name }),
    updateAccount: (id: string, name: string, number: string | null) =>
      call<void>('update_account', { id, name, number }),
    deleteAccount: (id: string) => call<number>('delete_account', { id }),
    setOwners: (accountId: string, ownerIds: string[]) => call<void>('set_owners', { accountId, ownerIds }),
    dbPath: () => call<string>('db_path'),
    exportToFolder: (accountIds: string[], start: string, end: string, dir: string) =>
      call<ExportResult[]>('export_ofx', { accountIds, start, end, dir }),
  };
}

async function tauriCall<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    // Commands reject with a plain message string.
    throw new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));
  }
}

function createTauriApi(): Api {
  const { exportToFolder, ...commands } = backed(tauriCall);
  return {
    ...commands,
    exportOfx: exportToFolder,
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
    uploadFile: () => Promise.reject(new Error('uploads are only used by the web version')),
    session: async () => ({ authRequired: false, authenticated: true, setupRequired: false, canChangePassword: false }),
    login: async () => {},
    logout: async () => {},
    setup: async () => {},
    changePassword: async () => {},
  };
}

let impl: Promise<Api> | null = null;
function resolve(): Promise<Api> {
  if (impl) return impl;
  impl =
    backend === 'tauri'
      ? Promise.resolve(createTauriApi())
      : backend === 'mock'
        ? import('./mock').then((m) => m.createMockApi())
        : import('./http').then((m) => m.createHttpApi());
  return impl;
}

/** Outside Tauri, calls go to the server (production build) or to in-memory demo data (dev, tests). */
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

/** Download link for an export produced by the server (`<session>/<file>`). */
export function downloadUrl(path: string): string {
  return `/api/download/${path.split('/').map(encodeURIComponent).join('/')}`;
}
