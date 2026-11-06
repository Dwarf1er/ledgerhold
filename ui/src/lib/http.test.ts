import { afterEach, describe, expect, it, vi } from 'vitest';
import { downloadUrl } from './api';
import { createHttpApi } from './http';
import { UnauthorizedError, setUnauthorizedHandler } from './unauthorized';

type Call = { url: string; init: RequestInit };

function server(handler: (url: string, init: RequestInit) => Response | Promise<Response>) {
  const calls: Call[] = [];
  const fetchFn = (async (url: string, init: RequestInit = {}) => {
    calls.push({ url, init });
    return handler(url, init);
  }) as unknown as typeof fetch;
  return { calls, api: createHttpApi(fetchFn) };
}

const ok = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });
const fail = (status: number, error: string) => new Response(JSON.stringify({ error }), { status });

afterEach(() => setUnauthorizedHandler(null));

describe('http backend', () => {
  it('posts commands as JSON to /api/rpc/<command> with camelCase args', async () => {
    const { api, calls } = server(() => ok({ accounts: [] }));
    await api.overview();
    expect(calls[0].url).toBe('/api/rpc/overview');
    expect(calls[0].init.method).toBe('POST');
    expect((calls[0].init.headers as Record<string, string>)['content-type']).toBe('application/json');
    expect(calls[0].init.body).toBe('{}');
    expect(calls[0].init.credentials).toBe('same-origin');

    await api.transactions('a1', '2026-01-01', null, 'cafe');
    expect(calls[1].url).toBe('/api/rpc/transactions');
    expect(JSON.parse(calls[1].init.body as string)).toEqual({ accountId: 'a1', from: '2026-01-01', to: null, search: 'cafe' });

    await api.setAnchor('a1', '2026-01-01', -2500, null);
    expect(JSON.parse(calls[2].init.body as string)).toEqual({ accountId: 'a1', date: '2026-01-01', amount: -2500, note: null });
  });

  it('turns server errors into messages the UI can show', async () => {
    const { api } = server(() => fail(422, 'account has no balance anchor; set a starting balance first'));
    await expect(api.balanceSeries('a', '2026-01-01', '2026-01-02')).rejects.toThrow('no balance anchor');
    const plain = server(() => new Response('<html>Bad gateway</html>', { status: 502, statusText: 'Bad Gateway' }));
    await expect(plain.api.overview()).rejects.toThrow('502 Bad Gateway');
  });

  it('reports an unreachable server in plain words', async () => {
    const api = createHttpApi((() => Promise.reject(new TypeError('Failed to fetch'))) as unknown as typeof fetch);
    await expect(api.overview()).rejects.toThrow(/Cannot reach the Ledgerhold server/);
  });

  it('a 401 calls the unauthorized handler once and throws a recognisable error', async () => {
    const handler = vi.fn();
    setUnauthorizedHandler(handler);
    const { api } = server(() => fail(401, 'sign in required'));
    await expect(api.overview()).rejects.toBeInstanceOf(UnauthorizedError);
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('a failed login surfaces the message without triggering the expiry handler', async () => {
    const handler = vi.fn();
    setUnauthorizedHandler(handler);
    const { api, calls } = server(() => fail(401, 'wrong password'));
    await expect(api.login('nope')).rejects.toThrow('wrong password');
    expect(handler).not.toHaveBeenCalled();
    expect(calls[0].url).toBe('/api/login');
    expect(JSON.parse(calls[0].init.body as string)).toEqual({ password: 'nope' });
  });

  it('session() reads /api/session without raising the expiry handler', async () => {
    const handler = vi.fn();
    setUnauthorizedHandler(handler);
    const { api, calls } = server(() => ok({ authRequired: true, authenticated: false }));
    expect(await api.session()).toEqual({ authRequired: true, authenticated: false });
    expect(calls[0].url).toBe('/api/session');
    expect(handler).not.toHaveBeenCalled();
  });

  it('uploads a file as the raw body with an encoded name and returns the server token', async () => {
    const { api, calls } = server(() => ok({ path: 'upload:1234/releve (1).csv', fileName: 'releve (1).csv' }));
    const file = new File(['a,b'], 'relevé (1).csv', { type: 'text/csv' });
    const path = await api.uploadFile(file);
    expect(path).toBe('upload:1234/releve (1).csv');
    expect(calls[0].url).toBe('/api/upload?name=relev%C3%A9%20(1).csv');
    expect((calls[0].init.headers as Record<string, string>)['content-type']).toBe('application/octet-stream');
    expect(calls[0].init.body).toBe(file);
  });

  it('exports into a server-side session instead of a folder', async () => {
    const { api, calls } = server(() => ok([{ path: 'sess/a.ofx', accountName: 'A', start: '2026-01-01', end: '2026-01-31', txnCount: 3, balance: null }]));
    const r = await api.exportOfx(['a1'], '2026-01-01', '2026-01-31', 'sess');
    expect(r[0].path).toBe('sess/a.ofx');
    expect(calls[0].url).toBe('/api/export');
    expect(JSON.parse(calls[0].init.body as string)).toEqual({ accountIds: ['a1'], start: '2026-01-01', end: '2026-01-31', session: 'sess' });
  });

  it('has no folder picker and signs out via /api/logout', async () => {
    const { api, calls } = server(() => new Response(null, { status: 204 }));
    expect(await api.pickFolder()).toBeNull();
    await api.logout();
    expect(calls[0].url).toBe('/api/logout');
  });
});

describe('downloadUrl', () => {
  it('encodes each path segment but keeps the separator', () => {
    expect(downloadUrl('6f1c/joint chequing-20260101.ofx')).toBe('/api/download/6f1c/joint%20chequing-20260101.ofx');
    expect(downloadUrl('s/all.zip')).toBe('/api/download/s/all.zip');
  });
});
