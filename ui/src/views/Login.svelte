<script lang="ts">
  import { api } from '../lib/api';
  import AuthCard from '../components/AuthCard.svelte';
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

<AuthCard title="Sign in" onsubmit={submit}>
  <p class="muted">Enter the household password.</p>
  <div class="field">
    <label for="pw">Password</label>
    <input id="pw" class="input" type="password" autocomplete="current-password" bind:value={password} data-autofocus />
  </div>
  {#if error}<Alert tone="error">{error}</Alert>{/if}
  <Button type="submit" variant="primary" loading={busy} disabled={!password}>Sign in</Button>
</AuthCard>
