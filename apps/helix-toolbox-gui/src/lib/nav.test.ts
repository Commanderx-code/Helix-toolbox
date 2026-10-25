// The sidebar's tool groups and the label above each screen's title.

import { describe, expect, it } from 'vitest';
import { TOOL_GROUPS, groupOf, same, sectionOf } from './nav';
import type { Screen } from './state.svelte';

describe('the tool groups', () => {
  it('list the tools as docs/GUI.md does', () => {
    expect(TOOL_GROUPS.map((group) => [group.name, group.tools.map((tool) => tool.label)])).toEqual([
      ['Health', ['Helix Doctor', 'Hardware', 'Performance', 'Services', 'Storage']],
      ['Upkeep', ['Updates', 'Drift', 'Reliability']],
      ['Safekeeping', ['Backups', 'Restic', 'Recovery', 'Migration', 'Offline bundles']],
      ['Workspace', ['Repositories', 'Home Manager', 'Configs', 'Desktop']],
    ]);
  });

  it('have each tool once, and ids of their own', () => {
    const kinds = TOOL_GROUPS.flatMap((group) => group.tools.map((tool) => tool.screen.kind));
    expect(new Set(kinds).size).toBe(kinds.length);
    const ids = TOOL_GROUPS.map((group) => group.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('say which group a tool is in, and none for the other screens', () => {
    expect(groupOf({ kind: 'restic' })?.id).toBe('backups');
    expect(groupOf({ kind: 'updates' })?.name).toBe('Upkeep');
    expect(groupOf({ kind: 'settings' })).toBeUndefined();
    expect(groupOf({ kind: 'area', index: 0 })).toBeUndefined();
  });
});

describe('a screen', () => {
  it('is the same screen by kind, and an area by its index too', () => {
    expect(same({ kind: 'doctor' }, { kind: 'doctor' })).toBe(true);
    expect(same({ kind: 'doctor' }, { kind: 'drift' })).toBe(false);
    expect(same({ kind: 'area', index: 2 }, { kind: 'area', index: 2 })).toBe(true);
    expect(same({ kind: 'area', index: 2 }, { kind: 'area', index: 3 })).toBe(false);
    expect(same({ kind: 'area', index: 2 }, { kind: 'plan' })).toBe(false);
  });

  it('carries where it sits in the sidebar above its title', () => {
    const sections = (screens: Screen[]) => screens.map(sectionOf);
    expect(sections([{ kind: 'dashboard' }])).toEqual(['Overview']);
    expect(sections([{ kind: 'favorites' }, { kind: 'plan' }, { kind: 'history' }])).toEqual(['Main menu', 'Main menu', 'Main menu']);
    expect(sections([{ kind: 'area', index: 3 }])).toEqual(['Catalog']);
    expect(sections([{ kind: 'settings' }])).toEqual(['Preferences']);
    for (const group of TOOL_GROUPS) {
      expect(sections(group.tools.map((tool) => tool.screen))).toEqual(group.tools.map(() => group.name));
    }
  });
});
