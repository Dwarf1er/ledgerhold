<script lang="ts">
  import { api } from '../lib/api';
  import Alert from '../components/Alert.svelte';
  import Button from '../components/Button.svelte';
  import { app, errMsg, refresh, session } from '../lib/state.svelte';

  let password = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!password) return;
    busy = true;
    error = null;
    try {
      await api.login(password);
      password = '';
      // Show "Loading…" (not a view with no data yet) while the household is fetched.
      app.loading = true;
      session.authenticated = true;
      await refresh();
    } catch (err) {
      error = errMsg(err);
      password = '';
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  <form class="card" onsubmit={submit}>
    <div class="brand"><span class="mark" aria-hidden="true"></span> Ledgerhold</div>
    <h1>Sign in</h1>
    <p class="muted">Enter the household password.</p>
    <div class="field">
      <label for="pw">Password</label>
      <input id="pw" class="input" type="password" autocomplete="current-password" bind:value={password} data-autofocus />
    </div>
    {#if error}<Alert tone="error">{error}</Alert>{/if}
    <Button type="submit" variant="primary" loading={busy} disabled={!password}>Sign in</Button>
  </form>
</div>

<style>
  .wrap {
    display: grid;
    place-items: center;
    min-height: 100%;
    padding: 24px;
  }
  form {
    display: grid;
    gap: 14px;
    width: 100%;
    max-width: 360px;
    padding: 28px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .mark {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    background: var(--fg);
    position: relative;
  }
  .mark::after {
    content: '';
    position: absolute;
    left: 4px;
    right: 4px;
    top: 5px;
    height: 2px;
    background: var(--bg);
    box-shadow: 0 4px 0 var(--bg);
  }
  h1 {
    font-size: 20px;
    line-height: 28px;
  }
</style>
