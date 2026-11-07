// The JS <-> Rust command boundary can't be exercised without a real window, so guard the
// part that breaks silently: every command the UI invokes must be registered in Rust, and
// every registered command should be used by the UI.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

// Tests run from the ui/ directory (jsdom's import.meta.url is not a file: URL).
const apiSrc = readFileSync(join(process.cwd(), 'src/lib/api.ts'), 'utf8');
const rustSrc = readFileSync(join(process.cwd(), '../src-tauri/src/lib.rs'), 'utf8');
const rpcSrc = readFileSync(join(process.cwd(), '../crates/app/src/rpc.rs'), 'utf8');
const serverSrc = readFileSync(join(process.cwd(), '../crates/server/src/lib.rs'), 'utf8');
const httpSrc = readFileSync(join(process.cwd(), 'src/lib/http.ts'), 'utf8');

const invoked = new Set([...apiSrc.matchAll(/call(?:<[^>]*>)?\('([a-z_]+)'/g)].map((m) => m[1]));
const handlerBlock = /generate_handler!\[([\s\S]*?)\]/.exec(rustSrc)?.[1] ?? '';
const registered = new Set(handlerBlock.split(',').map((s) => s.trim()).filter(Boolean));

const rpcCommands = new Set(
  [...(/COMMANDS: &\[&str\] = &\[([\s\S]*?)\];/.exec(rpcSrc)?.[1] ?? '').matchAll(/"([a-z_]+)"/g)].map((m) => m[1]),
);

describe('web server contract', () => {
  it('the server dispatcher handles exactly the commands the UI calls (export has its own endpoint)', () => {
    expect(rpcCommands.size).toBeGreaterThan(10);
    const ui = new Set([...invoked].filter((c) => c !== 'export_ofx'));
    expect([...ui].filter((c) => !rpcCommands.has(c))).toEqual([]);
    expect([...rpcCommands].filter((c) => !ui.has(c))).toEqual([]);
  });

  it('every non-command endpoint the browser client uses exists on the server', () => {
    for (const route of ['/api/login', '/api/logout', '/api/session', '/api/setup', '/api/password', '/api/upload', '/api/export']) {
      expect(httpSrc, `client uses ${route}`).toContain(route);
      expect(serverSrc, `server routes ${route}`).toContain(`"${route}"`);
    }
    expect(serverSrc).toContain('"/api/rpc/{command}"');
    expect(httpSrc).toContain('/api/rpc/');
    expect(serverSrc).toContain('"/api/download/{session}/{name}"');
    expect(apiSrc).toContain('/api/download/');
  });

  it('the zip download name matches on both sides', () => {
    expect(apiSrc + readFileSync(join(process.cwd(), 'src/components/ExportDialog.svelte'), 'utf8')).toContain('all.zip');
    expect(readFileSync(join(process.cwd(), '../crates/server/src/files.rs'), 'utf8')).toContain('"all.zip"');
  });
});

describe('tauri command contract', () => {
  it('finds both sides', () => {
    expect(invoked.size).toBeGreaterThan(10);
    expect(registered.size).toBeGreaterThan(10);
  });

  it('every command the UI calls is registered in Rust', () => {
    expect([...invoked].filter((c) => !registered.has(c))).toEqual([]);
  });

  it('every registered command is called by the UI', () => {
    expect([...registered].filter((c) => !invoked.has(c))).toEqual([]);
  });

  it('every registered command has a matching #[tauri::command] function', () => {
    for (const c of registered) expect(rustSrc).toMatch(new RegExp(`fn ${c}\\(`));
  });

  it('JS argument names are the camelCase of the Rust parameter names', () => {
    const camel = (s: string) => s.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
    for (const m of rustSrc.matchAll(/fn ([a-z_]+)\(([^)]*)\)/g)) {
      const [, name, params] = m;
      if (!invoked.has(name)) continue;
      const callSite = new RegExp(`call(?:<[^>]*>)?\\('${name}'(?:, \\{([^}]*)\\})?`).exec(apiSrc);
      const jsArgs = new Set((callSite?.[1] ?? '').split(',').map((s) => s.trim().split(':')[0].trim()).filter(Boolean));
      const rustArgs = params
        .split(/,(?![^<]*>)/)
        .map((p) => p.trim().split(':')[0].trim())
        .filter((p) => p && p !== 'state');
      expect([...jsArgs].sort(), `args of ${name}`).toEqual(rustArgs.map(camel).sort());
    }
  });
});
