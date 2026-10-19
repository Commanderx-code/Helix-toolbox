<script lang="ts" module>
  // Line icons on a 24-unit grid, drawn with the current color.
  const PATHS = {
    dashboard: 'M4 13a8 8 0 0 1 16 0M12 13l4-4M3 17h18',
    apps: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
    config: 'M4 7h10M18 7h2M4 17h4M12 17h8M14 4v6M8 14v6',
    maintenance: 'M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.6 2.6-2.4-.6-.6-2.4z',
    security: 'M12 3l8 3v6c0 4.5-3.4 8.3-8 9-4.6-.7-8-4.5-8-9V6z',
    profiles: 'M12 3l9 5-9 5-9-5zM3 13l9 5 9-5',
    star: 'M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z',
    plan: 'M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01',
    history: 'M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l3 3',
    search: 'M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4',
    plus: 'M12 5v14M5 12h14',
    minus: 'M5 12h14',
    check: 'M5 12l5 5L20 7',
    x: 'M6 6l12 12M18 6L6 18',
    play: 'M7 4v16l13-8z',
    stop: 'M6 6h12v12H6z',
    refresh: 'M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5',
    chevron: 'M9 6l6 6-6 6',
    terminal: 'M4 5h16v14H4zM7 9l3 3-3 3M12 15h5',
    warning: 'M12 3l10 18H2zM12 10v4M12 17.5v.01',
    info: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11v5M12 7.5v.01',
    package: 'M12 3l8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9',
    lock: 'M6 11h12v9H6zM8 11V8a4 4 0 0 1 8 0v3',
    folder: 'M3 6h6l2 2h10v11H3z',
    doctor: 'M12 4v16M4 12h16M7 7l10 10M17 7L7 17',
    backups: 'M4 7h16v4H4zM4 13h16v4H4zM7 9h.01M7 15h.01',
    drift: 'M4 7h11l-3-3M20 17H9l3 3M4 7l5 10M20 17l-5-10',
    flask: 'M9 3h6M10 3v6l-5 9a2 2 0 0 0 2 3h10a2 2 0 0 0 2-3l-5-9V3M7 15h10',
    chip: 'M7 7h10v10H7zM10 10h4v4h-4zM4 10h3M4 14h3M17 10h3M17 14h3M10 4v3M14 4v3M10 17v3M14 17v3',
    monitor: 'M3 5h18v11H3zM9 20h6M12 16v4',
    gear: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2',
    save: 'M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6',
    eye: 'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z',
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 18 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={PATHS[name]} />
</svg>
