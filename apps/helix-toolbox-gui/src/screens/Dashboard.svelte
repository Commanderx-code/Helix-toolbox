<script lang="ts">
  import emblem from '../../../../assets/icon.svg';
  import Icon, { type IconName } from '../components/Icon.svelte';
  import { toolbox } from '../lib/state.svelte';

  const counts = $derived.by(() => {
    const all = Object.values(toolbox.statuses);
    return {
      installed: all.filter((status) => status.kind === 'installed' || status.kind === 'managed').length,
      updates: all.filter((status) => status.kind === 'update').length,
      packages: all.length,
    };
  });

  const distribution = $derived(toolbox.about?.overview.rows.find(([label]) => label === 'Distribution')?.[1]);

  const applications = $derived(Math.max(0, toolbox.areas.findIndex((area) => area.id === 'applications')));

  const actions: { icon: IconName; title: string; detail: string; go: () => void }[] = [
    { icon: 'apps', title: 'Browse Applications', detail: 'Search and install tools', go: () => toolbox.open({ kind: 'area', index: applications }) },
    { icon: 'plan', title: 'Review the Install Plan', detail: 'See what is queued and run it', go: () => toolbox.open({ kind: 'plan' }) },
    { icon: 'check', title: 'Set up this machine', detail: 'A workstation to choose, as an Install Plan to review', go: () => (toolbox.wizard = true) },
    { icon: 'search', title: 'Find anything', detail: 'Screens, actions and entries (Ctrl K)', go: () => (toolbox.palette = true) },
    { icon: 'star', title: 'Favorites', detail: 'Pinned entries', go: () => toolbox.open({ kind: 'favorites' }) },
    { icon: 'history', title: 'View History', detail: 'Past runs and their results', go: () => toolbox.open({ kind: 'history' }) },
  ];
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Welcome back{toolbox.about?.person ? `, ${toolbox.about.person}` : ''}.</h1>
    </div>
    <div class="screen-buttons">
      <button class="button" disabled={toolbox.checking} onclick={() => toolbox.refresh()}>
        <Icon name="refresh" size={16} /> {toolbox.checking ? 'Checking…' : 'Check again'}
      </button>
      <button class="button primary" onclick={() => (toolbox.palette = true)}><Icon name="search" size={16} /> Find anything</button>
    </div>
  </div>

  {#if toolbox.about && !toolbox.about.command_line}
    <div class="notice">
      <Icon name="warning" />
      <span>The <span class="mono">helix-toolbox</span> command was not found beside this program or on the PATH. Built-in steps, such as the system report, need it.</span>
    </div>
  {/if}

  <section class="card hero" aria-labelledby="hero-title">
    <div class="pitch">
      <p class="kicker"><span class="dot"></span>Your Linux toolbox</p>
      <h2 id="hero-title">Build it, tune it,<br /><span class="accent">keep it running.</span></h2>
      <p class="blurb">
        Applications, configurations, maintenance and backups{distribution ? ` for ${distribution}` : ''}, with every change planned and shown to you before it runs.
      </p>
    </div>
    <div class="orbit" aria-hidden="true">
      <span class="ring outer"><span class="moon cyan"></span></span>
      <span class="ring middle"><span class="moon purple"></span></span>
      <span class="ring inner"></span>
      <img class="emblem" src={emblem} alt="" />
    </div>
  </section>

  <div class="stats">
    {@render stat('check', 'Installed', counts.installed, `of ${counts.packages} checked`, 'green')}
    {@render stat('refresh', 'Updates', counts.updates, counts.updates === 0 ? 'Nothing waiting' : 'Waiting to install', 'yellow')}
    {@render stat('plan', 'Install Plan', toolbox.queue.length, toolbox.queue.length === 0 ? 'Nothing queued' : 'Queued to run', 'cyan')}
    {@render stat('star', 'Favorites', toolbox.favorites.length, 'Pinned entries', 'purple')}
  </div>

  <div class="columns">
    <section class="card overview">
      <h2 class="card-title">System Overview</h2>
      <dl>
        {#each toolbox.about?.overview.rows ?? [] as [label, value] (label)}
          <dt>{label}</dt>
          <dd>{value}</dd>
        {/each}
      </dl>
    </section>

    <section class="card">
      <h2 class="card-title">Quick Actions</h2>
      <div class="actions">
        {#each actions as action (action.title)}
          <button class="action" onclick={action.go}>
            <span class="icon"><Icon name={action.icon} size={20} /></span>
            <span>
              <span class="title">{action.title}</span>
              <span class="detail">{action.detail}</span>
            </span>
            <Icon name="chevron" size={16} />
          </button>
        {/each}
      </div>
    </section>
  </div>
</div>

{#snippet stat(icon: IconName, label: string, value: number, detail: string, color: string)}
  <div class="card stat">
    <span class="tile" style:color="var(--{color})" style:background="color-mix(in srgb, var(--{color}) 12%, transparent)">
      <Icon name={icon} size={20} />
    </span>
    <div>
      <div class="label">{label}</div>
      <div class="value">
        {#if toolbox.checking && (label === 'Installed' || label === 'Updates') && counts.packages === 0}…{:else}{value}{/if}
      </div>
      <div class="faint small">{detail}</div>
    </div>
  </div>
{/snippet}

<style>
  .screen {
    display: flex;
    flex-direction: column;
    gap: 18px;
    height: 100%;
    padding: 20px 24px;
    overflow-y: auto;
  }

  .notice {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 10px 14px;
    border-radius: var(--radius);
    border: 1px solid color-mix(in srgb, var(--yellow) 50%, transparent);
    background: color-mix(in srgb, var(--yellow) 8%, transparent);
    color: var(--yellow);
  }

  .notice span {
    color: var(--text);
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 14px;
  }

  .screen-top {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
  }

  .hero {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    min-height: 220px;
    padding: 32px 44px;
    overflow: hidden;
    background:
      radial-gradient(circle at 82% 50%, color-mix(in srgb, var(--cyan) 10%, transparent), transparent 42%),
      linear-gradient(135deg, var(--panel-raised), var(--panel) 60%);
  }

  .kicker {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 12px;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--green);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--green);
    box-shadow: 0 0 8px var(--green);
  }

  .hero h2 {
    margin: 0;
    font-size: 36px;
    font-weight: 800;
    line-height: 1.1;
    letter-spacing: -0.02em;
  }

  .accent {
    color: var(--cyan);
  }

  .blurb {
    max-width: 560px;
    margin: 14px 0 0;
    color: var(--muted);
  }

  .orbit {
    position: relative;
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 190px;
    height: 190px;
  }

  .ring {
    position: absolute;
    border-radius: 50%;
    border: 1px solid color-mix(in srgb, var(--cyan) 28%, transparent);
  }

  .ring.outer {
    inset: 0;
    animation: spin 26s linear infinite;
  }

  .ring.middle {
    inset: 22px;
    border-color: color-mix(in srgb, var(--purple) 30%, transparent);
    animation: spin 18s linear infinite reverse;
  }

  .ring.inner {
    inset: 46px;
    box-shadow: 0 0 30px color-mix(in srgb, var(--cyan) 22%, transparent);
  }

  .moon {
    position: absolute;
    top: -4px;
    left: 50%;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .moon.cyan {
    background: var(--cyan);
    box-shadow: 0 0 10px var(--cyan);
  }

  .moon.purple {
    background: var(--purple);
    box-shadow: 0 0 10px var(--purple);
  }

  .emblem {
    position: relative;
    width: 72px;
    height: 72px;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .ring {
      animation: none;
    }
  }

  @media (max-width: 900px) {
    .orbit {
      display: none;
    }
  }

  .stat {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 18px;
  }

  .tile {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 42px;
    height: 42px;
    border-radius: var(--radius);
  }

  .label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--faint);
  }

  .value {
    font-size: 24px;
    font-weight: 800;
    line-height: 1.2;
  }

  .small {
    font-size: 12px;
  }

  .columns {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    gap: 14px;
    align-items: start;
  }

  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 8px 24px;
    margin: 0;
    padding: 12px 16px 16px;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
  }

  .action {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-small);
    border: 1px solid var(--line);
    background: var(--panel-raised);
    color: var(--faint);
    cursor: pointer;
    text-align: left;
  }

  .action:hover {
    border-color: var(--selected-line);
    background: var(--selected);
  }

  .icon {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-small);
    background: var(--bg);
    color: var(--cyan);
  }

  .title {
    display: block;
    font-weight: 700;
    color: var(--text);
  }

  .detail {
    display: block;
    font-size: 13px;
    color: var(--muted);
  }
</style>
