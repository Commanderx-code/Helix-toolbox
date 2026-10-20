// How well a query matches a name, the same ranking as the engine's
// helix_toolbox_core::search::score, so the palette and the areas agree
// with the terminal interface. Higher is better; null is no match.

const words = (text: string) => text.split(/[^\p{L}\p{N}]+/u).filter(Boolean);

/** Where `needle`'s characters start in `haystack`, in order, and how spread out they are. */
function subsequence(needle: string, haystack: string): [number, number] | null {
  const positions: number[] = [];
  let from = 0;
  const letters = [...haystack];
  for (const wanted of needle) {
    const found = letters.slice(from).indexOf(wanted);
    if (found < 0) return null;
    positions.push(from + found);
    from += found + 1;
  }
  if (positions.length === 0) return null;
  const first = positions[0];
  const last = positions[positions.length - 1];
  return [first, last - first + 1 - positions.length];
}

export function score(rawQuery: string, rawName: string, rawId: string, rawDescription: string): number | null {
  const query = rawQuery.trim().toLowerCase();
  if (query === '') return 0;
  const name = rawName.toLowerCase();
  const id = rawId.toLowerCase();
  const description = rawDescription.toLowerCase();
  if (name === query) return 1000;
  if (name.startsWith(query)) return 900;
  if (words(name).some((word) => word.startsWith(query))) return 800;
  if (name.includes(query)) return 700;
  if (id.includes(query)) return 650;
  // Several words, each found somewhere: "nvidia driver".
  const parts = words(query);
  if (parts.length > 1) {
    if (parts.every((part) => name.includes(part))) return 600;
    if (parts.every((part) => name.includes(part) || id.includes(part) || description.includes(part))) return 500;
  }
  // Letters in order, as in "vsc" for VS Code; the closer together, the better.
  const compact = query.replace(/\s+/g, '');
  const found = subsequence(compact, name);
  if (found && found[1] <= [...compact].length * 2) {
    const [first, spread] = found;
    const before = first === 0 ? '' : [...name][first - 1];
    const startsWord = first === 0 || !/[\p{L}\p{N}]/u.test(before);
    const bonus = (first === 0 ? 50 : 0) + (startsWord ? 30 : 0);
    return Math.max(0, 400 - spread * 10) + bonus;
  }
  if (description.includes(query)) return 90;
  return null;
}
