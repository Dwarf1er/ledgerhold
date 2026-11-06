<script lang="ts">
  import { formatMoney, formatSigned } from '../lib/format';
  let {
    minor,
    currency = 'CAD',
    signed = false,
    tint = false,
  }: { minor: number; currency?: string; signed?: boolean; tint?: boolean } = $props();
</script>

<span class="tnum" class:pos={tint && minor > 0}>{signed ? formatSigned(minor, currency) : formatMoney(minor, currency)}</span>

<style>
  .pos {
    color: var(--green);
  }
</style>
