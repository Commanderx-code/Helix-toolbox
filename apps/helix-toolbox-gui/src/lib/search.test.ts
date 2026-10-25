// The palette's ranking against the engine's: the same cases as the tests
// of helix_toolbox_core::search, and scores the Rust function gives.

import { describe, expect, it } from 'vitest';
import { score } from './search';

/** Names ranked for `query`, best first, as the Rust tests rank them. */
function rank(query: string, names: [string, string][]): string[] {
  return names
    .flatMap(([name, description]) => {
      const points = score(query, name, '', description);
      return points === null ? [] : [{ points, name }];
    })
    .sort((a, b) => b.points - a.points || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .map(({ name }) => name);
}

describe('the ranking', () => {
  // better_matches_come_first in crates/helix-toolbox-core/src/search.rs.
  it('puts better matches first', () => {
    const names: [string, string][] = [
      ['Ripgrep', 'Fast recursive search'],
      ['Neovim with Titus Config', 'Neovim, ripgrep and more'],
      ['VS Code', 'Visual Studio Code'],
      ['VS Codium', 'Code without telemetry'],
      ['Nvidia Drivers & Hardware Acceleration', "NVIDIA's driver"],
      ['General GPU Drivers', 'Drivers for any card'],
    ];
    expect(rank('ripgrep', names)).toEqual(['Ripgrep', 'Neovim with Titus Config']);
    expect(rank('code', names)[0]).toBe('VS Code');
    expect(rank('vsc', names)).toEqual(['VS Code', 'VS Codium']);
    expect(rank('rgp', names)[0]).toBe('Ripgrep');
    expect(rank('nvidia driver', names)).toEqual(['Nvidia Drivers & Hardware Acceleration']);
    expect(rank('drivers', names)).toHaveLength(2);
    expect(rank('zzz', names)).toEqual([]);
    expect(score('  ', 'Anything', '', '')).toBe(0);
    expect(score('RIP', 'ripgrep', '', '')).toBe(900);
    expect(score('titus', 'Neovim with Titus Config', '', '')).toBe(800);
  });

  // ids_and_descriptions_count_less_than_names.
  it('counts ids and descriptions less than names', () => {
    expect(score('pupgui', 'ProtonUp-Qt', 'net-davidotek-pupgui2', '')).toBe(650);
    expect(score('telemetry', 'VS Codium', '', 'Code without telemetry')).toBeLessThan(100);
  });
});

describe('each kind of match', () => {
  // [query, name, id, description, what helix_toolbox_core::search::score gives]
  const cases: [string, string, string, string, number | null][] = [
    ['Git', 'git', 'git', '', 1000],
    ['rip', 'Ripgrep', '', '', 900],
    ['titus', 'Neovim with Titus Config', '', '', 800],
    ['zip', '7-Zip', '', '', 800],
    ['grep', 'Ripgrep', '', '', 700],
    ['pupgui', 'ProtonUp-Qt', 'net-davidotek-pupgui2', '', 650],
    ['acceleration nvidia', 'Nvidia Drivers & Hardware Acceleration', '', '', 600],
    ['studio visual', 'VS Code', 'vscode', 'Visual Studio Code', 500],
    // Letters in order: from the name's start, from a word's, inside one.
    ['vsc', 'VS Code', '', '', 470],
    ['rgp', 'Ripgrep', '', '', 440],
    ['stdo', 'my studio', '', '', 410],
    ['bst', 'OBS Studio', '', '', 380],
    // Too scattered to count, so only the description can match.
    ['ab', 'a-----------b', '', '', null],
    ['ab', 'a-----------b', '', 'ab in here', 90],
    ['telemetry', 'VS Codium', '', 'Code without telemetry', 90],
    ['zzz', 'VS Code', '', '', null],
    ['  ', 'Anything', '', '', 0],
    // A vowel sign is part of a word, as Rust's is_alphanumeric has it.
    ['हिन', 'Editor हिन्दी', '', '', 800],
    ['िन', 'Editor हिन्दी', '', '', 700],
    ['ह द', 'हिन्दी', '', '', 600],
    ['नद', 'हिन्दी', '', '', 390],
  ];
  it.each(cases)('%j in %j scores as the engine does', (query, name, id, description, expected) => {
    expect(score(query, name, id, description)).toBe(expected);
  });
});
