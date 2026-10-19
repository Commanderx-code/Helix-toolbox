// What the flags column means: FLAGS_GUIDE in the terminal interface.

const MEANINGS: Record<string, string> = {
  D: 'disk changes, such as partitioning (privileged)',
  FI: 'Flatpak installation',
  FM: 'file changes',
  I: 'installation (privileged)',
  K: 'kernel changes (privileged)',
  MP: 'package manager actions',
  SI: 'full system installation',
  SS: 'service actions (privileged)',
  RP: 'package removal',
};

/** The words of a flags field and what each means. */
export function flags(field: string): { flag: string; meaning: string; high: boolean }[] {
  return field
    .split(/\s+/)
    .filter(Boolean)
    .map((flag) => {
      const base = flag.length > 1 && flag.startsWith('P') && MEANINGS[flag.slice(1)] ? flag.slice(1) : flag;
      const privileged = base !== flag;
      const meaning = MEANINGS[base] ?? 'unknown flag';
      return {
        flag,
        meaning: privileged ? `${meaning}, privileged` : meaning,
        high: ['D', 'K', 'SI'].includes(base),
      };
    });
}

export const KINDS: Record<string, string> = {
  package: 'Package',
  script: 'Script',
  command: 'Command',
  nix: 'Nix',
  native: 'Built in',
  profile: 'Profile',
};
