<script lang="ts">
  import { api } from '../lib/api';
  import Alert from '../components/Alert.svelte';
  import AuthCard from '../components/AuthCard.svelte';
  import Button from '../components/Button.svelte';
  import { app, errMsg, refresh, session } from '../lib/state.svelte';

  const MIN = 8;

  let code = $state('');
  let password = $state('');
  let confirm = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  const problem = $derived(
    password && password.length < MIN
      ? `Use at least ${MIN} characters.`
      : confirm && confirm !== password
        ? 'The two passwords do not match.'
        : null,
  );
  const ready = $derived(code.trim() !== '' && password.length >= MIN && confirm === password);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!ready) return;
    busy = true;
    error = null;
    try {
      await api.setup(code.trim(), password);
      password = confirm = code = '';
      // The server signs us in as part of setup.
      app.loading = true;
      session.setupRequired = false;
      session.authRequired = true;
      session.canChangePassword = true;
      session.authenticated = true;
      await refresh();
    } catch (err) {
      error = errMsg(err);
    } finally {
      busy = false;
    }
  }
</script>

<AuthCard title="Choose a password" onsubmit={submit}>
  <p class="muted">
    This server has no password yet. Everyone in the household will share the one you choose here.
  </p>
  <div class="field">
    <label for="code">Setup code</label>
    <input id="code" class="input mono" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="XXXX-XXXX-XXXX" bind:value={code} data-autofocus />
    <p class="hint">Printed in the server log when it started, e.g. <span class="mono">docker compose logs ledgerhold</span>.</p>
  </div>
  <div class="field">
    <label for="new">Password</label>
    <input id="new" class="input" type="password" autocomplete="new-password" bind:value={password} />
  </div>
  <div class="field">
    <label for="again">Confirm password</label>
    <input id="again" class="input" type="password" autocomplete="new-password" bind:value={confirm} />
  </div>
  {#if problem}<p class="hint warn">{problem}</p>{/if}
  {#if error}<Alert tone="error">{error}</Alert>{/if}
  <Button type="submit" variant="primary" loading={busy} disabled={!ready}>Set password and continue</Button>
</AuthCard>

<style>
  .hint {
    margin-top: 6px;
    font-size: 12px;
    color: var(--fg-3);
  }
  .warn {
    margin: 0;
    color: var(--fg-2);
  }
</style>
