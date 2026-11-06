<script lang="ts">
  import { initials } from '../lib/format';
  let { name, size = 22 }: { name: string; size?: number } = $props();
</script>

<span class="avatar" style="width:{size}px;height:{size}px;font-size:{Math.round(size * 0.42)}px" title={name}>
  {initials(name)}
</span>

<style>
  .avatar {
    display: inline-grid;
    place-items: center;
    border-radius: 50%;
    background: var(--bg-3);
    color: var(--fg-2);
    border: 1px solid var(--bg);
    box-shadow: 0 0 0 1px var(--line);
    font-weight: 600;
    letter-spacing: 0;
  }
</style>
