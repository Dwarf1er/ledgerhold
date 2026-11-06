import { api } from './api';
import type { Overview } from './api';

export const app = $state<{ overview: Overview | null; loading: boolean; error: string | null; person: string }>({
  overview: null,
  loading: true,
  error: null,
  /** Household filter: 'all' or a person id. */
  person: 'all',
});

export function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export async function refresh(): Promise<void> {
  try {
    app.overview = await api.overview();
    app.error = null;
  } catch (e) {
    app.error = errMsg(e);
  } finally {
    app.loading = false;
  }
}

// ---- toasts ------------------------------------------------------------------

export type ToastKind = 'success' | 'error' | 'info';
export const toasts = $state<{ id: number; message: string; kind: ToastKind }[]>([]);
let toastSeq = 0;

export function toast(message: string, kind: ToastKind = 'info', ms = 4500): void {
  const id = ++toastSeq;
  toasts.push({ id, message, kind });
  setTimeout(() => dismissToast(id), kind === 'error' ? ms * 2 : ms);
}

export function dismissToast(id: number): void {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}

// ---- theme -------------------------------------------------------------------

export type ThemeMode = 'system' | 'light' | 'dark';
const KEY = 'ledgerhold.theme';

function readTheme(): ThemeMode {
  try {
    const v = localStorage.getItem(KEY);
    if (v === 'light' || v === 'dark' || v === 'system') return v;
  } catch {
    /* storage unavailable */
  }
  return 'system';
}

export const theme = $state<{ mode: ThemeMode }>({ mode: readTheme() });

export function setTheme(mode: ThemeMode): void {
  theme.mode = mode;
  try {
    localStorage.setItem(KEY, mode);
  } catch {
    /* storage unavailable */
  }
  applyTheme();
}

export function applyTheme(): void {
  const root = document.documentElement;
  if (theme.mode === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', theme.mode);
}

// ---- last used export folder -----------------------------------------------------

export function lastExportDir(): string | null {
  try {
    return localStorage.getItem('ledgerhold.exportDir');
  } catch {
    return null;
  }
}
export function rememberExportDir(dir: string): void {
  try {
    localStorage.setItem('ledgerhold.exportDir', dir);
  } catch {
    /* storage unavailable */
  }
}
