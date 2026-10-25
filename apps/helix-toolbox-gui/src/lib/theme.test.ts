// @vitest-environment jsdom
// The window's theme on the document, and the copy kept for the first paint.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { applyRemembered, applyTheme } from './theme';

const root = document.documentElement;

afterEach(() => {
  vi.restoreAllMocks();
  delete root.dataset.theme;
  localStorage.clear();
});

describe('the theme', () => {
  it('is set on the document and remembered', () => {
    applyTheme('dark');
    expect(root.dataset.theme).toBe('dark');
    expect(localStorage.getItem('helix-toolbox-theme')).toBe('dark');
    applyTheme('light');
    expect(root.dataset.theme).toBe('light');
  });

  it("follows the desktop's preference when the system's is chosen", () => {
    applyTheme('dark');
    applyTheme('system');
    expect(root.dataset.theme).toBeUndefined();
    expect(localStorage.getItem('helix-toolbox-theme')).toBe('system');
  });

  it('is put back from the copy before the setting is read', () => {
    localStorage.setItem('helix-toolbox-theme', 'light');
    applyRemembered();
    expect(root.dataset.theme).toBe('light');
  });

  it('leaves the desktop its preference when nothing or something else is remembered', () => {
    applyRemembered();
    expect(root.dataset.theme).toBeUndefined();
    for (const kept of ['system', 'sepia']) {
      localStorage.setItem('helix-toolbox-theme', kept);
      applyRemembered();
      expect(root.dataset.theme).toBeUndefined();
    }
  });

  it('applies without storage', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('storage is off', 'SecurityError');
    });
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new DOMException('storage is off', 'SecurityError');
    });
    expect(() => applyTheme('dark')).not.toThrow();
    expect(root.dataset.theme).toBe('dark');
    expect(() => applyRemembered()).not.toThrow();
  });
});
