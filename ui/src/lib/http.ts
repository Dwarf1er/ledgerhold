// Browser backend: talks to ledgerhold-server (crates/server) over fetch.
import { backed } from './api';
import type { Api, ExportResult, SessionInfo } from './api';
import { UnauthorizedError, notifyUnauthorized } from './unauthorized';

type Fetch = typeof fetch;

async function errorFrom(res: Response): Promise<Error> {
  let message = `${res.status} ${res.statusText}`.trim();
  try {
    const body = await res.json();
    if (typeof body?.error === 'string') message = body.error;
  } catch {
    /* not JSON */
  }
  return new Error(message);
}

export function createHttpApi(fetchFn: Fetch = (...a) => fetch(...a)): Api {
  async function send(path: string, init: RequestInit): Promise<Response> {
    let res: Response;
    try {
      res = await fetchFn(path, { credentials: 'same-origin', ...init });
    } catch {
      throw new Error('Cannot reach the Ledgerhold server. Check your connection and try again.');
    }
    if (res.status === 401 && path !== '/api/login') {
      notifyUnauthorized();
      throw new UnauthorizedError();
    }
    if (!res.ok) throw await errorFrom(res);
    return res;
  }

  const json = (body: unknown): RequestInit => ({
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body ?? {}),
  });

  const call = async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    const res = await send(`/api/rpc/${cmd}`, json(args));
    return (await res.json()) as T;
  };

  const { exportToFolder: _unused, ...commands } = backed(call);
  void _unused;

  async function upload(file: File): Promise<string> {
    const res = await send(`/api/upload?name=${encodeURIComponent(file.name)}`, {
      method: 'POST',
      headers: { 'content-type': 'application/octet-stream' },
      body: file,
    });
    return ((await res.json()) as { path: string }).path;
  }

  return {
    ...commands,
    // In a browser the "folder" is an export session id; files stay on the server until downloaded.
    async exportOfx(accountIds, start, end, session): Promise<ExportResult[]> {
      const res = await send('/api/export', json({ accountIds, start, end, session }));
      return (await res.json()) as ExportResult[];
    },
    pickFile() {
      return new Promise<string | null>((resolve, reject) => {
        const input = document.createElement('input');
        input.type = 'file';
        input.accept = '.csv,.pdf,.ofx,.qfx,.qbo';
        input.onchange = () => {
          const f = input.files?.[0];
          if (!f) return resolve(null);
          upload(f).then(resolve, reject);
        };
        input.addEventListener('cancel', () => resolve(null));
        input.click();
      });
    },
    pickFolder: async () => null,
    uploadFile: upload,
    async session(): Promise<SessionInfo> {
      const res = await fetchFn('/api/session', { credentials: 'same-origin' });
      if (!res.ok) throw await errorFrom(res);
      return (await res.json()) as SessionInfo;
    },
    async login(password) {
      await send('/api/login', json({ password }));
    },
    async logout() {
      await send('/api/logout', json({}));
    },
  };
}
