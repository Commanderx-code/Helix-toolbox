// Outside Tauri, as in `npm run dev` and these tests, the commands answer
// from mock.ts.

import { describe, expect, it } from 'vitest';
import * as api from './api';

describe('the commands outside Tauri', () => {
  it('answer with the sample data', async () => {
    expect((await api.areas()).map((area) => area.id)).toEqual(['applications', 'configurations', 'maintenance', 'profiles']);
    expect(await api.pin('git', true)).toEqual(['neovim', 'ripgrep', 'git']);
  });

  it("plan a profile's entries", async () => {
    const prepared = await api.prepare(['developer-basics']);
    expect(prepared.kind === 'ready' && prepared.lines.map((line) => line.name)).toEqual(['Git', 'Neovim', 'Ripgrep', 'fzf']);
  });
});

describe('output bytes', () => {
  it('come from the base64 of an event', () => {
    expect([...api.bytes(btoa('ok\r\n'))]).toEqual([111, 107, 13, 10]);
    expect([...api.bytes(btoa('\xff\x00'))]).toEqual([255, 0]);
    expect(api.bytes('')).toHaveLength(0);
  });
});
