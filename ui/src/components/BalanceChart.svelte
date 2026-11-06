<script lang="ts">
  import type { BalancePoint } from '../lib/api';
  import { formatDate, formatMoney, formatShortDate } from '../lib/format';

  let {
    points,
    currency = 'CAD',
    anchorDates = [],
    height = 220,
  }: { points: BalancePoint[]; currency?: string; anchorDates?: string[]; height?: number } = $props();

  let width = $state(640);
  let hover = $state<number | null>(null);

  const PAD = { top: 12, right: 8, bottom: 22, left: 8 };
  const compact = $derived(
    new Intl.NumberFormat('en-CA', { style: 'currency', currency, notation: 'compact', maximumFractionDigits: 1 }),
  );

  const scale = $derived.by(() => {
    if (!points.length) return { lo: 0, hi: 1 };
    const vals = points.map((p) => p.balance);
    let lo = Math.min(...vals);
    let hi = Math.max(...vals);
    if (lo === hi) {
      lo -= 100;
      hi += 100;
    }
    const pad = (hi - lo) * 0.08;
    return { lo: lo - pad, hi: hi + pad };
  });

  const innerW = $derived(Math.max(10, width - PAD.left - PAD.right));
  const innerH = $derived(height - PAD.top - PAD.bottom);
  const x = (i: number) => PAD.left + (points.length <= 1 ? innerW / 2 : (i / (points.length - 1)) * innerW);
  const y = (v: number) => PAD.top + (1 - (v - scale.lo) / (scale.hi - scale.lo)) * innerH;

  const line = $derived(points.map((p, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)} ${y(p.balance).toFixed(1)}`).join(' '));
  const area = $derived(
    points.length > 1 ? `${line} L${x(points.length - 1).toFixed(1)} ${PAD.top + innerH} L${x(0).toFixed(1)} ${PAD.top + innerH} Z` : '',
  );
  const zeroY = $derived(scale.lo < 0 && scale.hi > 0 ? y(0) : null);
  const yTicks = $derived([scale.hi, (scale.hi + scale.lo) / 2, scale.lo].map((v) => ({ v, y: y(v) })));
  const markers = $derived(
    anchorDates.map((d) => points.findIndex((p) => p.date === d)).filter((i) => i >= 0),
  );
  const xTicks = $derived.by(() => {
    if (points.length < 2) return [];
    const n = Math.min(4, points.length);
    return Array.from({ length: n }, (_, k) => Math.round((k / (n - 1)) * (points.length - 1)));
  });

  function move(e: PointerEvent) {
    const rect = (e.currentTarget as SVGElement).getBoundingClientRect();
    const px = e.clientX - rect.left;
    const i = Math.round(((px - PAD.left) / innerW) * (points.length - 1));
    hover = Math.min(points.length - 1, Math.max(0, i));
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      const d = e.key === 'ArrowLeft' ? -1 : 1;
      hover = Math.min(points.length - 1, Math.max(0, (hover ?? points.length - 1) + d));
    } else if (e.key === 'Escape') hover = null;
  }

  const shown = $derived(hover !== null ? points[hover] : null);
  const summary = $derived(
    points.length
      ? `Balance from ${formatDate(points[0].date)} (${formatMoney(points[0].balance, currency)}) to ${formatDate(points[points.length - 1].date)} (${formatMoney(points[points.length - 1].balance, currency)})`
      : 'No data',
  );
</script>

<div class="wrap" bind:clientWidth={width}>
  {#if points.length === 0}
    <div class="empty" style="height:{height}px">No balance data for this period</div>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <svg
      {width}
      {height}
      role="img"
      aria-label={summary}
      tabindex="0"
      onpointermove={move}
      onpointerleave={() => (hover = null)}
      onkeydown={key}
      onblur={() => (hover = null)}
    >
      <defs>
        <linearGradient id="fill" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0" stop-color="var(--fg)" stop-opacity="0.09" />
          <stop offset="1" stop-color="var(--fg)" stop-opacity="0" />
        </linearGradient>
      </defs>
      {#each yTicks as t}
        <line x1={PAD.left} x2={width - PAD.right} y1={t.y} y2={t.y} class="grid" />
        <text x={PAD.left} y={t.y - 4} class="tick">{compact.format(t.v / 100)}</text>
      {/each}
      {#if zeroY !== null}
        <line x1={PAD.left} x2={width - PAD.right} y1={zeroY} y2={zeroY} class="zero" />
      {/if}
      {#if area}<path d={area} fill="url(#fill)" />{/if}
      <path d={line} class="line" />
      {#each markers as i}
        <circle cx={x(i)} cy={y(points[i].balance)} r="3.5" class="anchor" />
      {/each}
      {#each xTicks as i, k}
        <text x={x(i)} y={height - 6} class="tick" text-anchor={k === 0 ? 'start' : k === xTicks.length - 1 ? 'end' : 'middle'}>
          {formatShortDate(points[i].date)}
        </text>
      {/each}
      {#if hover !== null}
        <line x1={x(hover)} x2={x(hover)} y1={PAD.top} y2={PAD.top + innerH} class="cross" />
        <circle cx={x(hover)} cy={y(points[hover].balance)} r="4" class="dot" />
      {/if}
    </svg>
    {#if shown && hover !== null}
      <div class="tip" style="left:{Math.min(Math.max(x(hover), 70), width - 70)}px">
        <span class="muted">{formatDate(shown.date)}</span>
        <strong class="tnum">{formatMoney(shown.balance, currency)}</strong>
      </div>
    {/if}
  {/if}
</div>

<style>
  .wrap {
    position: relative;
    width: 100%;
  }
  svg {
    display: block;
    overflow: visible;
    border-radius: var(--radius);
  }
  .grid {
    stroke: var(--line);
    stroke-width: 1;
  }
  .zero {
    stroke: var(--fg-4);
    stroke-width: 1;
    stroke-dasharray: 3 3;
  }
  .line {
    fill: none;
    stroke: var(--fg);
    stroke-width: 1.75;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .tick {
    fill: var(--fg-4);
    font-size: 11px;
    font-family: var(--mono);
  }
  .anchor {
    fill: var(--bg);
    stroke: var(--fg);
    stroke-width: 1.5;
  }
  .cross {
    stroke: var(--fg-4);
    stroke-width: 1;
  }
  .dot {
    fill: var(--fg);
    stroke: var(--bg);
    stroke-width: 2;
  }
  .tip {
    position: absolute;
    top: 0;
    transform: translate(-50%, -100%);
    display: grid;
    gap: 0;
    padding: 6px 10px;
    background: var(--bg);
    border-radius: var(--radius);
    box-shadow: var(--menu-shadow);
    font-size: 12px;
    white-space: nowrap;
    pointer-events: none;
  }
  .empty {
    display: grid;
    place-items: center;
    color: var(--fg-3);
    border: 1px dashed var(--line-2);
    border-radius: var(--radius-lg);
  }
</style>
