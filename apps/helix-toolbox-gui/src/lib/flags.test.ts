// What the flags column says, as FLAGS_GUIDE in the terminal interface.

import { describe, expect, it } from 'vitest';
import { flags } from './flags';

describe('the flags column', () => {
  it('names each flag of the field, in order', () => {
    expect(flags('I FM MP')).toEqual([
      { flag: 'I', meaning: 'installation (privileged)', high: false },
      { flag: 'FM', meaning: 'file changes', high: false },
      { flag: 'MP', meaning: 'package manager actions', high: false },
    ]);
  });

  it('reads P before a flag as the same action, privileged', () => {
    expect(flags('PFM')).toEqual([{ flag: 'PFM', meaning: 'file changes, privileged', high: false }]);
    expect(flags('PK')[0].high).toBe(true);
  });

  it('marks disk, kernel and full system changes high, as a plan rates them', () => {
    expect(flags('SI D K SS RP FI').map(({ flag, high }) => [flag, high])).toEqual([
      ['SI', true],
      ['D', true],
      ['K', true],
      ['SS', false],
      ['RP', false],
      ['FI', false],
    ]);
  });

  it('says it does not know a flag, and a lone P', () => {
    expect(flags('P MP ZZ PZZ').map(({ flag, meaning }) => [flag, meaning])).toEqual([
      ['P', 'unknown flag'],
      ['MP', 'package manager actions'],
      ['ZZ', 'unknown flag'],
      ['PZZ', 'unknown flag'],
    ]);
  });

  it('has nothing to say about an empty field', () => {
    expect(flags('')).toEqual([]);
    expect(flags('   ')).toEqual([]);
    expect(flags('  I   FM ').map(({ flag }) => flag)).toEqual(['I', 'FM']);
  });
});
