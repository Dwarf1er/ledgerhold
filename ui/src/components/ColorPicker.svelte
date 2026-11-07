<script lang="ts">
  import { PERSON_COLORS } from '../lib/colors';

  let {
    value = $bindable(''),
    label = 'Colour',
    size = 22,
    onchange,
  }: { value?: string; label?: string; size?: number; onchange?: (key: string) => void } = $props();
</script>

<div class="swatches" role="radiogroup" aria-label={label}>
  {#each PERSON_COLORS as c (c.key)}
    <button
      type="button"
      role="radio"
      aria-checked={value === c.key}
      aria-label={c.label}
      title={c.label}
      class:on={value === c.key}
      style="background:{c.hex};width:{size}px;height:{size}px"
      onclick={() => { value = c.key; onchange?.(c.key); }}
    ></button>
  {/each}
</div>

<style>
  .swatches {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  button {
    border: 0;
    border-radius: 50%;
    padding: 0;
    cursor: pointer;
    box-shadow: 0 0 0 1px var(--line-2);
  }
  button:hover {
    box-shadow: 0 0 0 1px var(--fg-3);
  }
  button.on {
    box-shadow: 0 0 0 2px var(--bg), 0 0 0 4px var(--fg);
  }
  button:focus-visible {
    box-shadow: var(--focus);
  }
</style>
