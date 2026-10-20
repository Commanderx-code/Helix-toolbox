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

  // axe-core, which scripts/gui-smoke.sh puts in front of this script when
  // npm has installed it: what it finds wrong (labels, roles, contrast) on
  // the window as it is now fails the step.
  async function accessible(where) {
    if (!window.axe) return;
    // The window makes scrolling regions reachable within 300 ms.
    await sleep(500);
    const { violations } = await window.axe.run(document, { resultTypes: ['violations'] });
    if (violations.length > 0) {
      const first = violations[0];
      throw new Error(
        `${where}: ${violations.map((found) => found.id).join(', ')}; ${first.help} at ${first.nodes[0]?.target.join(' ')}`,
      );
    }
  }

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

  await step('a theme from Settings applies at once and is saved', async () => {
    const background = () => getComputedStyle(document.documentElement).getPropertyValue('--bg').trim();
    const before = background();
    (await until('the Light choice', () => document.querySelector('input[name="theme"][value="light"]'))).click();
    await until('the light theme', () => document.documentElement.dataset.theme === 'light');
    await accessible('Settings in the light theme');
    const light = background();
    if (light !== '#f3f5fa') throw new Error(`the light background is ${light}`);
    document.querySelector('input[name="theme"][value="system"]').click();
    await until('the desktop\'s theme', () => !document.documentElement.dataset.theme);
    document.querySelector('input[name="theme"][value="dark"]').click();
    await until('the dark theme', () => document.documentElement.dataset.theme === 'dark');
    await accessible('Settings in the dark theme');
    document.querySelector('input[name="theme"][value="system"]').click();
    await until('the desktop\'s theme again', () => !document.documentElement.dataset.theme);
    return `${before} → ${light} → ${background()}`;
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

  await step('the keyboard moves through a list, queues with Space and keeps the focus in dialogs', async () => {
    await click('Tools', document.querySelector('nav'));
    await until('the Tools screen', () => screenTitle().startsWith('Tools'));
    const marker = await until('Marker', () => all('.list .select').find((node) => node.textContent.includes('Marker')));
    marker.focus();
    key(marker, 'ArrowDown');
    const next = document.activeElement;
    if (next === marker || !next.matches('.select, .group')) throw new Error('ArrowDown did not move on');
    if (next.matches('.select')) {
      const name = next.querySelector('.name').textContent.trim();
      await until(`${name} selected`, () => withText('aside h2', name));
    }
    key(next, 'ArrowUp');
    if (document.activeElement !== marker) throw new Error('ArrowUp did not come back');
    await until('Marker selected', () => withText('aside h2', 'Marker'));
    const queued = () => withText('nav .item', 'Install Plan')?.querySelector('.count')?.textContent.trim() ?? '0';
    const before = queued();
    key(marker, ' ');
    await until('Marker queued', () => queued() !== before);
    key(marker, ' ');
    await until('Marker taken out', () => queued() === before);
    // The palette takes the focus, keeps Tab inside and gives it back.
    key(window, 'k', { ctrlKey: true });
    const input = await until('the palette', () => document.querySelector('[aria-label="Command palette"] input'));
    await until('the focus in the palette', () => document.activeElement === input);
    await accessible('the palette');
    const controls = all('[aria-label="Command palette"] button, [aria-label="Command palette"] input');
    controls[controls.length - 1].focus();
    key(document.activeElement, 'Tab');
    if (document.activeElement !== controls[0]) throw new Error('Tab left the palette');
    key(input, 'Escape');
    await until('the palette to close', () => !document.querySelector('[aria-label="Command palette"]'));
    await until('the focus back on Marker', () => document.activeElement === marker);
  });

  const screens = ['Updates','Backups', 'Drift', 'Reliability', 'Hardware', 'Desktop', 'History', 'Favorites', 'Dashboard'];
  for (const name of screens) {
    await step(`the ${name} screen opens`, async () => {
      await click(name, document.querySelector('nav'));
      await until(`the ${name} screen`, () => screenTitle().startsWith(name));
      // What it reads has arrived, or it said what went wrong.
      await until('the screen settled', () => !withText('.empty-note', '…') && !withText('.empty-note', 'Reading'), 30000);
      await accessible(name);
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
    // The focus starts on Cancel, so Enter never runs a plan by accident.
    await until('the focus on Cancel', () => document.activeElement?.textContent.trim() === 'Cancel');
    await accessible('the confirmation');
    // The change preview first, then back to the same confirmation.
    await click('What it changes', dialog);
    await until('the step', () => withText('[role="dialog"] section h3', 'Marker'), 30000);
    const changes = all('[role="dialog"] .change').length;
    await accessible('the change preview');
    if (changes === 0) throw new Error('the change preview lists nothing for the marker');
    await click('Back to the confirmation', dialog);
    const primary = await until('the confirm button', () => dialog.querySelector('.button.primary'));
    primary.click();
    const result = await until('the result', () => withText('.good, .bad', 'Result:'), 60000);
    if (!result.classList.contains('good')) throw new Error(result.textContent.trim());
    await accessible('the finished run');
    await click('Close');
    await until('the run panel to close', () => !document.querySelector('#run-title'));
    return `${changes} changes previewed; ${result.textContent.trim()}`;
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

  await step('the Offline bundles screen reads the plan and checks a folder', async () => {
    await click('Offline bundles', document.querySelector('nav'));
    await until('the bundles screen', () => screenTitle() === 'Offline bundles');
    await click('Check what it needs');
    const marker = await until('the contents', () => withText('.entries li', 'Marker'), 60000);
    const state = marker.querySelector('.pill').textContent.trim();
    if (state !== 'Needs the network') throw new Error(`the marker script is "${state}"`);
    await type(all('.field input')[1], '/nonexistent/helix-toolbox-bundle');
    await click('Check it');
    const said = await until('the check', () => all('.part')[1].querySelector('.said.bad'), 30000);
    await accessible('Offline bundles');
    if (!said.textContent.includes('/nonexistent')) throw new Error(said.textContent.trim());
    return `Marker: ${state}; a missing folder: ${said.textContent.trim()}`;
  });

  await trace(window.axe ? `axe-core ${window.axe.version} checked the screens` : 'axe-core was not loaded; nothing was checked for accessibility');
  await trace(failed ? 'DONE with failures' : 'DONE');
  await sleep(300);
  await invoke('quit');
})();
