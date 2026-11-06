// Behaviour that only exists in the browser (self-hosted) build.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import App from './App.svelte';
import { __setApi, capabilities } from './lib/api';
import type { Api } from './lib/api';
import { createMockApi } from './lib/mock';
import { parseHash, router } from './lib/router.svelte';
import { app, markSignedOut, session, toasts } from './lib/state.svelte';

function go(hash: string) {
  location.hash = hash;
  router.route = parseHash(hash);
}
const main = () => within(document.querySelector('main') as HTMLElement);
/** `main` only exists once the shell has rendered (e.g. after signing in). */
async function shell() {
  await waitFor(() => expect(document.querySelector('main')).toBeTruthy());
  return main();
}

/** A backend that requires the password "secret". */
function protectedApi(): Api & { logins: string[] } {
  const base = createMockApi();
  let signedIn = false;
  const logins: string[] = [];
  return {
    ...base,
    logins,
    session: async () => ({ authRequired: true, authenticated: signedIn }),
    login: async (pw: string) => {
      logins.push(pw);
      if (pw !== 'secret') throw new Error('wrong password');
      signedIn = true;
    },
    logout: async () => {
      signedIn = false;
    },
  };
}

beforeEach(() => {
  app.overview = null;
  app.loading = true;
  app.error = null;
  app.person = 'all';
  session.checked = false;
  session.authRequired = false;
  session.authenticated = true;
  toasts.length = 0;
  capabilities.pickFolder = true;
  go('#/');
});

afterEach(() => {
  cleanup();
  capabilities.pickFolder = true;
});

describe('sign-in', () => {
  it('shows the sign-in screen first and nothing of the household until the password is right', async () => {
    __setApi(protectedApi());
    render(App);
    expect(await screen.findByRole('heading', { name: 'Sign in' })).toBeTruthy();
    expect(screen.queryByText('Joint chequing')).toBeNull();
    expect(document.querySelector('aside')).toBeNull();

    const password = screen.getByLabelText('Password');
    await fireEvent.input(password, { target: { value: 'nope' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    expect(await screen.findByText('wrong password')).toBeTruthy();
    expect((screen.getByLabelText('Password') as HTMLInputElement).value).toBe('');

    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'secret' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    expect(await (await shell()).findByText('Joint chequing')).toBeTruthy();
  });

  it('the sign-in button stays disabled until something is typed', async () => {
    __setApi(protectedApi());
    render(App);
    await screen.findByRole('heading', { name: 'Sign in' });
    expect((screen.getByRole('button', { name: 'Sign in' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('is skipped entirely when the server has no password', async () => {
    __setApi(createMockApi());
    render(App);
    expect(await main().findByText('Joint chequing')).toBeTruthy();
    expect(screen.queryByRole('heading', { name: 'Sign in' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Sign out' })).toBeNull();
  });

  it('an expired session drops the cached data and returns to the sign-in screen', async () => {
    const api = protectedApi();
    __setApi(api);
    render(App);
    await fireEvent.input(await screen.findByLabelText('Password'), { target: { value: 'secret' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    await (await shell()).findByText('Joint chequing');

    markSignedOut(); // what the HTTP client does on a 401
    expect(await screen.findByRole('heading', { name: 'Sign in' })).toBeTruthy();
    expect(app.overview).toBeNull();
  });

  it('can sign out from the sidebar', async () => {
    const api = protectedApi();
    __setApi(api);
    render(App);
    await fireEvent.input(await screen.findByLabelText('Password'), { target: { value: 'secret' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    await (await shell()).findByText('Joint chequing');
    await fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
    expect(await screen.findByRole('heading', { name: 'Sign in' })).toBeTruthy();
  });

  it('a server that cannot be reached shows the reason, not a blank page', async () => {
    const api = { ...createMockApi(), session: async () => { throw new Error('Cannot reach the Ledgerhold server.'); } };
    __setApi(api);
    render(App);
    expect(await screen.findByText(/Cannot reach the Ledgerhold server/)).toBeTruthy();
  });
});

describe('importing in a browser', () => {
  it('accepts a dropped file by uploading it', async () => {
    const api = createMockApi();
    const uploaded: string[] = [];
    api.uploadFile = async (f) => {
      uploaded.push(f.name);
      return `upload:abc/${f.name}`;
    };
    __setApi(api);
    go('#/import?account=a-joint');
    render(App);
    const drop = await main().findByRole('button', { name: /Choose a statement/ });
    expect(drop.textContent).toContain('drop it here');
    const file = new File(['<OFX></OFX>'], 'janvier.ofx');
    await fireEvent.drop(drop, { dataTransfer: { files: [file] } });
    expect(await main().findByText('janvier.ofx')).toBeTruthy();
    expect(uploaded).toEqual(['janvier.ofx']);
    expect((main().getByRole('button', { name: 'Preview import' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('shows an upload failure instead of silently doing nothing', async () => {
    const api = createMockApi();
    api.uploadFile = async () => {
      throw new Error('the file is empty');
    };
    __setApi(api);
    go('#/import?account=a-joint');
    render(App);
    const drop = await main().findByRole('button', { name: /Choose a statement/ });
    await fireEvent.drop(drop, { dataTransfer: { files: [new File([''], 'x.csv')] } });
    expect(await main().findByText('the file is empty')).toBeTruthy();
  });
});

describe('exporting in a browser', () => {
  beforeEach(() => {
    capabilities.pickFolder = false;
    __setApi(createMockApi());
  });

  it('needs no folder and offers per-file download links', async () => {
    go('#/account/a-joint');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Export OFX/ }));
    const dlg = within(await screen.findByRole('dialog'));
    expect(dlg.queryByText('Destination')).toBeNull();
    const exportBtn = dlg.getByRole('button', { name: /^Export$/ }) as HTMLButtonElement;
    expect(exportBtn.disabled).toBe(false);
    await fireEvent.click(exportBtn);
    expect(await dlg.findByText(/1 file is ready to download/)).toBeTruthy();
    const link = dlg.getByRole('link', { name: /joint-chequing-\d{8}-\d{8}\.ofx/ }) as HTMLAnchorElement;
    expect(link.getAttribute('href')).toMatch(/^\/api\/download\/[0-9a-f-]{36}\/joint-chequing-/);
    expect(link.hasAttribute('download')).toBe(true);
    expect(dlg.queryByRole('link', { name: /Download all/ })).toBeNull();
  });

  it('every-month export offers a single zip of all files', async () => {
    go('#/account/a-joint');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Export OFX/ }));
    const dlg = within(await screen.findByRole('dialog'));
    await fireEvent.click(dlg.getByRole('radio', { name: 'Every month' }));
    await fireEvent.click(dlg.getByRole('button', { name: /^Export$/ }));
    const zip = (await dlg.findByRole('link', { name: /Download all as \.zip/ })) as HTMLAnchorElement;
    expect(zip.getAttribute('href')).toMatch(/^\/api\/download\/[0-9a-f-]{36}\/all\.zip$/);
    const files = dlg.getAllByRole('link').filter((a) => /\.ofx$/.test(a.textContent ?? ''));
    expect(files.length).toBeGreaterThan(3);
    // All files share the one session, so the zip contains all of them.
    const sessions = new Set(files.map((a) => (a.getAttribute('href') ?? '').split('/')[3]));
    expect(sessions.size).toBe(1);
    expect([...sessions][0]).toBe(zip.getAttribute('href')!.split('/')[3]);
  });

  it('the desktop flavour still asks for a folder', async () => {
    capabilities.pickFolder = true;
    go('#/account/a-joint');
    render(App);
    await fireEvent.click(await main().findByRole('button', { name: /Export OFX/ }));
    const dlg = within(await screen.findByRole('dialog'));
    expect(dlg.getByText('Destination')).toBeTruthy();
    expect((dlg.getByRole('button', { name: /^Export$/ }) as HTMLButtonElement).disabled).toBe(true);
    await waitFor(() => expect(dlg.getByText('Choose a destination folder')).toBeTruthy());
  });
});
