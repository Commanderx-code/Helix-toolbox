// The sidebar's tools, grouped, and the label each screen's header carries.

import type { IconName } from '../components/Icon.svelte';
import type { Screen } from './state.svelte';

export type Tool = { screen: Screen; icon: IconName; label: string };
export type ToolGroup = { id: string; name: string; icon: IconName; tools: Tool[] };

/** More tools, in groups the sidebar opens and closes. Settings stays on its own. */
export const TOOL_GROUPS: ToolGroup[] = [
  {
    id: 'health',
    name: 'Health',
    icon: 'doctor',
    tools: [
      { screen: { kind: 'doctor' }, icon: 'doctor', label: 'Helix Doctor' },
      { screen: { kind: 'hardware' }, icon: 'chip', label: 'Hardware' },
      { screen: { kind: 'performance' }, icon: 'chart', label: 'Performance' },
      { screen: { kind: 'services' }, icon: 'gear', label: 'Services' },
      { screen: { kind: 'storage' }, icon: 'disk', label: 'Storage' },
    ],
  },
  {
    id: 'updates',
    name: 'Upkeep',
    icon: 'refresh',
    tools: [
      { screen: { kind: 'updates' }, icon: 'refresh', label: 'Updates' },
      { screen: { kind: 'drift' }, icon: 'drift', label: 'Drift' },
      { screen: { kind: 'reliability' }, icon: 'flask', label: 'Reliability' },
    ],
  },
  {
    id: 'backups',
    name: 'Safekeeping',
    icon: 'backups',
    tools: [
      { screen: { kind: 'backups' }, icon: 'backups', label: 'Backups' },
      { screen: { kind: 'restic' }, icon: 'archive', label: 'Restic' },
      { screen: { kind: 'recovery' }, icon: 'lifebuoy', label: 'Recovery' },
      { screen: { kind: 'migration' }, icon: 'swap', label: 'Migration' },
      { screen: { kind: 'bundles' }, icon: 'package', label: 'Offline bundles' },
    ],
  },
  {
    id: 'workspace',
    name: 'Workspace',
    icon: 'branch',
    tools: [
      { screen: { kind: 'repositories' }, icon: 'branch', label: 'Repositories' },
      { screen: { kind: 'home-manager' }, icon: 'home', label: 'Home Manager' },
      { screen: { kind: 'configs' }, icon: 'sliders', label: 'Configs' },
      { screen: { kind: 'desktop' }, icon: 'monitor', label: 'Desktop' },
    ],
  },
];

export const same = (a: Screen, b: Screen) =>
  a.kind === b.kind && (a.kind !== 'area' || (b.kind === 'area' && a.index === b.index));

/** The group a tool screen is in, if it is one. */
export const groupOf = (screen: Screen) =>
  TOOL_GROUPS.find((group) => group.tools.some((tool) => same(tool.screen, screen)));

/** The small label above a screen's title: where it sits in the sidebar. */
export function sectionOf(screen: Screen): string {
  switch (screen.kind) {
    case 'dashboard':
      return 'Overview';
    case 'favorites':
    case 'plan':
    case 'history':
      return 'Main menu';
    case 'area':
      return 'Catalog';
    case 'settings':
      return 'Preferences';
    default:
      return groupOf(screen)?.name ?? 'More tools';
  }
}
