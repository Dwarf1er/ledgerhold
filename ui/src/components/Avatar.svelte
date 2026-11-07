<script lang="ts">
  import { colorHex } from '../lib/colors';
  import { initials } from '../lib/format';
  let { name, color = null, size = 22 }: { name: string; color?: string | null; size?: number } = $props();
</script>

<span
  class="avatar"
  class:plain={!color}
  style="width:{size}px;height:{size}px;font-size:{Math.round(size * 0.42)}px{color ? `;background:${colorHex(color)}` : ''}"
  title={name}
>
  {initials(name)}
</span>

<style>
  .avatar {
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 50%;
    background: var(--bg-3);
    color: #fff;
    border: 1px solid var(--bg);
    box-shadow: 0 0 0 1px var(--line);
    font-weight: 600;
    letter-spacing: 0;
  }
  .plain {
    color: var(--fg-2);
  }
</style>
