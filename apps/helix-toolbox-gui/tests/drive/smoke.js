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

  const screens = ['Updates','Backups', 'Drift', 'Reliability', 'Hardware', 'Services', 'Storage', 'Performance', 'Recovery', 'Migration', 'Desktop', 'History', 'Favorites', 'Dashboard'];
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
    return text.split(' is saved')[0] + ' is saved';
  });

  await step('your profile is exported, deleted and imported again', async () => {
    await click('Profiles', document.querySelector('nav'));
    await until('the Profiles area', () => screenTitle() === 'Profiles');
    if (!button('Import a profile')) throw new Error('no Import a profile button');
    const row = await until('the saved profile', () => withText('.list .select', 'Smoke profile'));
    row.click();
    const details = await until('the details', () => withText('aside h2', 'Smoke profile')?.closest('aside'));
    if (!button('Export', details)) throw new Error('no Export button');
    await accessible('your profile');
    // The file chooser is the desktop's, out of reach here: the commands
    // take the file instead, in the scratch settings folder.
    const { path } = await invoke('read_settings');
    const file = path.replace(/settings\.toml$/, 'exported.toml');
    const exported = await invoke('export_profile', { id: 'profile-smoke-profile', path: file });
    if (!exported?.includes('is exported to')) throw new Error(exported);
    await click('Delete', details);
    const input = await until('the confirmation', () => document.querySelector('#prompt-title') && document.querySelector('[role="dialog"] input'));
    await type(input, 'DELETE');
    key(input, 'Enter');
    await until('the notice', () => withText('#notice-title', 'Profile deleted'));
    await click('OK');
    await until('the profile gone', () => !withText('.list .select', 'Smoke profile'));
    const imported = await invoke('import_profile', { path: file });
    if (!imported?.includes('is imported as')) throw new Error(imported);
    const areas = await invoke('areas');
    if (!JSON.stringify(areas).includes('"profile-smoke-profile"')) throw new Error('the imported profile is not in Profiles');
    return 'exported, deleted, imported';
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

  await step('an installed package entry offers removal, which the engine plans', async () => {
    key(window, 'k', { ctrlKey: true });
    const input = await until('the palette', () => document.querySelector('[aria-label="Command palette"] input'));
    await type(input, 'removable sample');
    key(input, 'Enter');
    const details = await until('the details', () => withText('aside h2', 'Removable Sample')?.closest('aside'));
    // The button waits for the status check to find it installed.
    await click('Remove', details);
    const title = await until('the confirmation', () => document.querySelector('#confirm-title'));
    const dialog = title.closest('[role="dialog"]');
    const said = dialog.textContent;
    if (!said.includes('not by a package Helix Toolbox knows')) throw new Error(said.trim());
    await accessible('the removal');
    await click('OK', dialog);
    await until('the confirmation to close', () => !document.querySelector('#confirm-title'));
    return `${title.textContent.trim()}: installed, but not by a package Helix Toolbox knows`;
  });

  await step('a bug report is gathered from the command line and copied', async () => {
    await click('Settings', document.querySelector('nav'));
    await until('Settings', () => screenTitle().startsWith('Settings'));
    await click('Make a bug report');
    const dialog = await until('the report', () => document.querySelector('#report-title')?.closest('[role="dialog"]'));
    const text = await until('the report text', () => dialog.querySelector('pre')?.textContent || dialog.querySelector('[role="alert"]')?.textContent, 30000);
    if (!text.includes("the smoke test's stand-in")) throw new Error(text);
    if (!text.includes('### The window') || !text.includes('Helix Toolbox window:')) throw new Error(`no window section: ${text}`);
    if (!text.includes('- Settings: ~/.config/')) throw new Error(`the home folder is not shown as ~: ${text}`);
    await accessible('the bug report');
    await click('Copy', dialog);
    await until('copied', () => withText('[role="status"]', 'Copied'));
    await click('Close', dialog);
    await until('the report to close', () => !document.querySelector('#report-title'));
    return text.split('\n').find((line) => line.startsWith('- WebKitGTK')) ?? 'gathered and copied';
  });

  await step('a service change and a cleanup are planned and reviewed, and nothing runs', async () => {
    await click('Services', document.querySelector('nav'));
    await until('Services', () => screenTitle().startsWith('Services'));
    const row = await until('a unit', () => document.querySelector('.row-item.unit'), 30000);
    row.click();
    const aside = await until('its details', () => document.querySelector('aside h2')?.closest('aside'));
    const change = await until('a change button', () => [...aside.querySelectorAll('.actions .button')].find((button) => !button.disabled));
    const asked = change.textContent.trim();
    change.click();
    const title = await until('the review', () => document.querySelector('#confirm-title'));
    const dialog = title.closest('[role="dialog"]');
    if (!dialog.textContent.includes('systemctl')) throw new Error(dialog.textContent);
    await accessible('a service change under review');
    await click('Cancel', dialog);
    await until('the review to close', () => !document.querySelector('#confirm-title'));
    await click('Storage', document.querySelector('nav'));
    await until('Storage', () => screenTitle().startsWith('Storage'));
    await until('the storage read', () => document.querySelector('.items') || withText('.empty-note', 'Error'), 60000);
    const clean = [...document.querySelectorAll('.items .button')].find((button) => !button.disabled);
    if (clean) {
      clean.click();
      const cleaning = await until('the cleanup review', () => document.querySelector('#confirm-title')?.closest('[role="dialog"]'));
      await accessible('a cleanup under review');
      await click('Cancel', cleaning);
      await until('the review to close', () => !document.querySelector('#confirm-title'));
    }
    return `${asked} reviewed${clean ? ', and a cleanup' : ''}; nothing ran`;
  });

  await step('a boot hint and the recovery checks are reviewed, and nothing runs', async () => {
    await click('Performance', document.querySelector('nav'));
    await until('Performance', () => screenTitle().startsWith('Performance'));
    await until('the boot read', () => withText('.part h2', 'This boot'), 60000);
    const off = await until('the hint to turn waiting online off', () => [...document.querySelectorAll('.hints .button')].find((button) => button.textContent.includes('Turn it off')), 30000);
    off.click();
    const review = await until('the review', () => document.querySelector('#confirm-title')?.closest('[role="dialog"]'));
    if (!review.textContent.includes('NetworkManager-wait-online.service')) throw new Error(review.textContent);
    await accessible('a boot hint under review');
    await click('Cancel', review);
    await until('the review to close', () => !document.querySelector('#confirm-title'));
    await click('Recovery', document.querySelector('nav'));
    await until('Recovery', () => screenTitle().startsWith('Recovery'));
    await until('the checks', () => withText('.part h2', 'Configuration backups'), 30000);
    return 'the wait-online hint was reviewed; recovery checks shown; nothing ran';
  });

  await step('this machine is exported, and a migration from another distribution is read and reviewed, and nothing runs', async () => {
    await click('Migration', document.querySelector('nav'));
    await until('Migration', () => screenTitle().startsWith('Migration'));
    await until('this machine read', () => button('Export'), 60000);
    // As with the profile, the file chooser is out of reach: the commands
    // take the file, in the scratch settings folder, where
    // scripts/gui-smoke.sh also wrote a migration made on Fedora.
    const { path } = await invoke('read_settings');
    const exported = await invoke('export_migration', { path: path.replace(/settings\.toml$/, 'migration.toml') });
    if (!exported?.includes('are written to')) throw new Error(exported);
    const input = await until('the file field', () => document.querySelector('.field input'));
    await type(input, path.replace(/settings\.toml$/, 'from-fedora.toml'));
    await click('Read it');
    const part = await until('the migration read', () => withText('.part h2', 'From Fedora Linux 43')?.closest('.part'), 30000);
    const install = part.querySelector('[aria-label="To install"]');
    const unavailable = part.querySelector('[aria-label="Cannot be installed here"]');
    if (!install?.textContent.includes('Marker')) throw new Error(`to install: ${install?.textContent.trim()}`);
    // The Flatpak is installed where Flatpak is; the workflow's runner has
    // none, and the fixture catalog no entry that installs it.
    if (!install.textContent.includes('com.example.Tool') && !unavailable?.textContent.includes('Flatpak is not installed here')) {
      throw new Error(`the Flatpak is neither to install nor said to need Flatpak: ${part.textContent.trim()}`);
    }
    if (!unavailable?.textContent.includes('not-in-this-catalog')) {
      throw new Error('the unknown entry is not listed as unavailable');
    }
    if (!withText('.items li', 'htop')) throw new Error('the Fedora package is not listed');
    await accessible('a migration read here');
    await click('Install', part);
    const review = await until('the review', () => document.querySelector('#confirm-title')?.closest('[role="dialog"]'), 30000);
    if (!review.textContent.includes('Marker')) throw new Error(review.textContent);
    await accessible('a migration under review');
    await click(button('Cancel', review) ? 'Cancel' : 'OK', review);
    await until('the review to close', () => !document.querySelector('#confirm-title'));
    return `exported; ${install.querySelectorAll('li').length} to install reviewed; nothing ran`;
  });

  await step('an entry is checked now, without changing anything', async () => {
    key(window, 'k', { ctrlKey: true });
    const input = await until('the palette', () => document.querySelector('[aria-label="Command palette"] input'));
    await type(input, 'marker');
    key(input, 'Enter');
    const details = await until('the details', () => withText('aside h2', 'Marker')?.closest('aside'));
    await click('Check it now', details);
    const told = await until('the checks', () => withText('#notice-title', 'Checks: Marker')?.closest('[role="dialog"]'));
    const text = told.querySelector('pre')?.textContent ?? '';
    if (!text.includes('nothing to check') && !text.includes('as it should be')) throw new Error(text);
    await accessible('the checks');
    await click('OK', told);
    return text.split('\n')[0];
  });

  await step('a run that worked becomes a hardware report, and its issue opens', async () => {
    await click('History', document.querySelector('nav'));
    await until('History', () => screenTitle().startsWith('History'));
    await click('Report it worked', await until('a run that worked', () => withText('tr', 'Marker')));
    const dialog = await until('the report', () => document.querySelector('#worked-title')?.closest('[role="dialog"]'));
    const text = dialog.querySelector('pre')?.textContent ?? '';
    if (!text.includes('[[hardware]]') || !text.includes('entry = "marker"')) throw new Error(text);
    await accessible('the hardware report');
    await click('Open the issue', dialog);
    await until('opened', () => withText('[role="status"]', 'Open in your browser'));
    await click('Close', dialog);
    await until('the report to close', () => !document.querySelector('#worked-title'));
    return text.split('\n').find((line) => line.startsWith('System:')) ?? 'reported';
  });

  await step('update notifications turn on and off from Settings', async () => {
    await click('Settings', document.querySelector('nav'));
    await until('Settings', () => screenTitle().startsWith('Settings'));
    const box = () => withText('label.toggle', 'Tell me when updates are waiting')?.querySelector('input');
    (await until('the notifications switch', () => box() && !box().disabled && box())).click();
    const told = await until('what turning on did', () => document.querySelector('#notice-title')?.closest('[role="dialog"]'));
    if (!told.textContent.includes('every 6 hours')) throw new Error(told.textContent);
    await accessible('Settings with notifications on');
    await click('OK', told);
    if ((await invoke('read_settings')).notifications !== true) throw new Error('they are not on');
    await until('the switch on', () => box()?.checked && !box().disabled);
    box().click();
    const start = Date.now();
    while ((await invoke('read_settings')).notifications !== false) {
      if (Date.now() - start > 20000) throw new Error('they are not off');
      await sleep(200);
    }
    await until('the switch off', () => box() && !box().checked && !box().disabled);
    // Health problems alone keep the timer, and turning them off removes it.
    const health = () => withText('label.toggle', 'Helix Doctor finds a new problem')?.querySelector('input');
    (await until('the health switch', () => health() && !health().disabled && health())).click();
    const healthTold = await until('what turning on did', () => document.querySelector('#notice-title')?.closest('[role="dialog"]'));
    if (!healthTold.textContent.includes('Helix Doctor finds a new problem')) throw new Error(healthTold.textContent);
    await click('OK', healthTold);
    const watched = await invoke('read_settings');
    if (watched.health !== true || watched.notifications !== false) throw new Error(JSON.stringify(watched));
    await accessible('Settings with health notifications on');
    await until('the health switch on', () => health()?.checked && !health().disabled);
    health().click();
    const off = Date.now();
    while ((await invoke('read_settings')).health !== false) {
      if (Date.now() - off > 20000) throw new Error('health notifications are not off');
      await sleep(200);
    }
    return 'updates on, then off; health problems on, then off';
  });

  await trace(window.axe ? `axe-core ${window.axe.version} checked the screens` : 'axe-core was not loaded; nothing was checked for accessibility');
  await trace(failed ? 'DONE with failures' : 'DONE');
  await sleep(300);
  await invoke('quit');
})();
