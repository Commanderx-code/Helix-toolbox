// The smoke test's drive script: run in a debug build of the window by
// `--drive` once the page has loaded (scripts/gui-smoke.sh), against the
// fixture catalog with the wizard's workstations from setup.toml. It works
// the interface through the DOM, the way a person would, and reports on
// standard error: "drive: PASS …" for each step, "drive: FAIL …" for one
// that went wrong, "drive: DONE" at the end, then quits the program.

(async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const trace = (message) => invoke('trace', { message });
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  let failed = false;

  /** Wait until `find` returns something, at most `timeout` ms. */
  async function until(what, find, timeout = 20000) {
    const start = Date.now();
    while (Date.now() - start < timeout) {
      const found = find();
      if (found) return found;
      await sleep(100);
    }
    throw new Error(`timed out waiting for ${what}`);
  }
  const all = (selector) => [...document.querySelectorAll(selector)];
  const withText = (selector, text) => all(selector).find((node) => node.textContent.trim().includes(text));
  const button = (text, within = document) =>
    [...within.querySelectorAll('button')].find((node) => node.textContent.trim().includes(text));
  async function click(text, within = document) {
    const found = await until(`a "${text}" button`, () => button(text, within));
    found.click();
    await sleep(150);
  }
  async function type(input, text) {
    input.focus();
    input.value = text;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(200);
  }
  const key = (target, key, init = {}) =>
    target.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }));
  const screenTitle = () => document.querySelector('.screen-title')?.textContent.trim() ?? '';

  async function step(name, work) {
    try {
      const detail = await work();
      await trace(`PASS ${name}${detail ? `: ${detail}` : ''}`);
    } catch (error) {
      failed = true;
      await trace(`FAIL ${name}: ${error.message ?? error}`);
    }
  }

  await step('the window loaded', async () => {
    await until('the sidebar', () => all('nav .item').length >= 8);
    await until('the dashboard', () => screenTitle() === 'Dashboard');
    return `${all('nav .item').length} sidebar items`;
  });

  await step('the wizard opens by itself on the first start', async () => {
    const title = await until('the welcome', () => withText('[role="dialog"] h2', 'Welcome to Helix Toolbox'));
    await until('the machine', () => all('[role="dialog"] dl dt').length >= 5);
    return title.textContent.trim();
  });

  await step('the wizard offers the workstations and their entries', async () => {
    await click('Choose a workstation');
    await until('the workstations', () => all('.choice').length === 2);
    await click('Smoke test');
    const picks = await until('the picks', () => (all('.pick').length >= 4 ? all('.pick') : null));
    const names = picks.map((pick) => pick.querySelector('.name').textContent.trim());
    const unavailable = picks.filter((pick) => pick.classList.contains('unavailable')).length;
    const ticked = picks.filter((pick) => pick.querySelector('input').checked).length;
    if (!names.includes('Marker')) throw new Error(`the picks are ${names.join(', ')}`);
    if (unavailable !== 1) throw new Error(`${unavailable} entries unavailable, not the one missing from the catalog`);
    if (ticked < 2) throw new Error(`only ${ticked} ticked`);
    return `${names.join(', ')}; ${ticked} ticked, ${unavailable} unavailable`;
  });

  await step('the wizard fills the Install Plan and runs nothing', async () => {
    await click('to the Install Plan');
    const notice = await until('the notice', () => withText('#notice-title', 'Install Plan'));
    const text = notice.parentElement.querySelector('pre').textContent;
    if (!text.includes('Added to the Install Plan')) throw new Error(text);
    await click('OK');
    await until('the plan screen', () => screenTitle().startsWith('Install Plan'));
    const queued = await until('the queued rows', () => (all('.rows .row').length >= 2 ? all('.rows .row').length : null));
    return `${queued} queued; ${text.split('\n')[0]}`;
  });

  await step('the wizard opens again from Settings', async () => {
    await click('Settings', document.querySelector('nav'));
    await until('the settings screen', () => screenTitle() === 'Settings');
    await click('Set up this machine');
    await until('the machine page', () => withText('[role="dialog"] h2', 'This machine'));
    await click('Close');
    await until('the wizard to close', () => !document.querySelector('[role="dialog"][data-page]'));
  });

  await step('Ctrl-K opens the palette, which finds an entry', async () => {
    // Whatever an earlier step left open closes first.
    key(window, 'Escape');
    await sleep(200);
    key(window, 'k', { ctrlKey: true });
    const input = await until('the palette', () => document.querySelector('[aria-label="Command palette"] input'));
    const listed = all('[aria-label="Command palette"] .row').length;
    if (listed < 15) throw new Error(`${listed} choices listed before typing`);
    await type(input, 'marker');
    const first = await until('a result', () => document.querySelector('[aria-label="Command palette"] .row.current .label'));
    if (!first.textContent.includes('Marker')) throw new Error(`the first result is ${first.textContent}`);
    key(input, 'Enter');
    await until('the entry shown', () => withText('aside h2', 'Marker'));
    return `${listed} listed, first result "${first.textContent.trim()}"`;
  });

  await step('the palette opens a screen and runs an action', async () => {
    key(window, 'k', { ctrlKey: true });
    const input = await until('the palette', () => document.querySelector('[aria-label="Command palette"] input'));
    await type(input, 'open doctor');
    key(input, 'Enter');
    await until('Helix Doctor', () => screenTitle() === 'Helix Doctor');
    key(window, 'k', { ctrlKey: true });
    const again = await until('the palette', () => document.querySelector('[aria-label="Command palette"] input'));
    await type(again, 'clear the install plan');
    key(again, 'Enter');
    await until('the plan emptied', () => !withText('nav .item', 'Install Plan')?.querySelector('.count'));
    key(window, 'Escape');
  });

  const screens = ['Updates', 'Backups', 'Drift', 'Reliability', 'Hardware', 'Desktop', 'History', 'Favorites', 'Dashboard'];
  for (const name of screens) {
    await step(`the ${name} screen opens`, async () => {
      await click(name, document.querySelector('nav'));
      await until(`the ${name} screen`, () => screenTitle().startsWith(name));
      // What it reads has arrived, or it said what went wrong.
      await until('the screen settled', () => !withText('.empty-note', '…') && !withText('.empty-note', 'Reading'), 30000);
      return screenTitle();
    });
  }

  await step('an entry runs through the confirmation and the terminal', async () => {
    key(window, 'k', { ctrlKey: true });
    const input = await until('the palette', () => document.querySelector('[aria-label="Command palette"] input'));
    await type(input, 'marker');
    key(input, 'Enter');
    const details = await until('the details', () => withText('aside h2', 'Marker')?.closest('aside'));
    await click('Run now', details);
    const title = await until('the confirmation', () => document.querySelector('#confirm-title'));
    const dialog = title.closest('[role="dialog"]');
    const primary = await until('the confirm button', () => dialog.querySelector('.button.primary'));
    primary.click();
    const result = await until('the result', () => withText('.good, .bad', 'Result:'), 60000);
    if (!result.classList.contains('good')) throw new Error(result.textContent.trim());
    await click('Close');
    await until('the run panel to close', () => !document.querySelector('#run-title'));
    return result.textContent.trim();
  });

  await step('the plan is saved as a profile', async () => {
    const details = await until('the details', () => withText('aside h2', 'Marker')?.closest('aside'));
    await click('Add to plan', details);
    await click('Install Plan', document.querySelector('nav'));
    await until('the plan screen', () => screenTitle().startsWith('Install Plan'));
    await click('Save as profile');
    const input = await until('the name prompt', () => document.querySelector('#prompt-title') && document.querySelector('[role="dialog"] input'));
    await type(input, 'Smoke profile');
    key(input, 'Enter');
    const notice = await until('the notice', () => withText('#notice-title', 'Profile saved'));
    const text = notice.parentElement.querySelector('pre').textContent;
    if (!text.includes('is saved')) throw new Error(text);
    await click('OK');
    // The fixture catalog has no Profiles area to show it in; the runner
    // checks the saved file.
    return text.split(' is saved')[0] + ' is saved';
  });

  await trace(failed ? 'DONE with failures' : 'DONE');
  await sleep(300);
  await invoke('quit');
})();
