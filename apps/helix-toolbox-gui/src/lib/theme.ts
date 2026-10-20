// The window's theme: the desktop's light or dark preference, or one of
// them always, as Settings chose. settings.toml keeps the choice; this
// window's local storage keeps a copy, so the first paint already has it.

import type { WindowTheme } from './types';

const KEY = 'helix-toolbox-theme';

/** Use this theme: "system" leaves it to the desktop's preference. */
export function applyTheme(theme: WindowTheme) {
  const root = document.documentElement;
  if (theme === 'system') delete root.dataset.theme;
  else root.dataset.theme = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // Without storage the saved setting still applies once it is read.
  }
}

/** The theme last used, before the saved setting has been read. */
export function applyRemembered() {
  try {
    const theme = localStorage.getItem(KEY);
    if (theme === 'dark' || theme === 'light') document.documentElement.dataset.theme = theme;
  } catch {
    // Nothing remembered: the desktop's preference until the setting arrives.
  }
}
