<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { Device, HardwareView } from '../lib/types';

  let found = $state<HardwareView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);
  let showDevices = $state<'none' | 'pci' | 'usb'>('none');

  async function read() {
    reading = true;
    try {
      found = await api.hardware();
      problem = null;
    } catch (error) {
      problem = String(error);
    } finally {
      reading = false;
    }
  }

  read();

  async function updateFirmware(index: number, device: string) {
    const word = await toolbox.ask(
      'Firmware update',
      `Updating firmware can leave ${device} unusable if it is interrupted. Keep the machine on mains power. Type FIRMWARE to see the plan:`,
      'FIRMWARE',
      'FIRMWARE',
    );
    if (word !== 'FIRMWARE') return;
    await toolbox.reviewWith(() => api.prepareFirmware(index));
  }

  const devices = $derived<Device[]>(showDevices === 'pci' ? (found?.pci ?? []) : showDevices === 'usb' ? (found?.usb ?? []) : []);
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Hardware</h1>
      <p class="screen-subtitle">Firmware, disks, temperatures and batteries, read from the system; changes ask first.</p>
    </div>
    <button class="button" disabled={reading} onclick={read}><Icon name="refresh" size={16} /> {reading ? 'Reading…' : 'Read again'}</button>
  </div>

  <div class="pane card content">
    {#if problem}
      <p class="empty-note bad">{problem}</p>
    {:else if found === null}
      <p class="empty-note">Reading the hardware…</p>
    {:else}
      <section>
        <h2 class="section">Firmware</h2>
        {#each found.firmware as line, index (index)}<p class="muted">{line}</p>{/each}
        {#each found.updates as update, index (index)}
          <div class="line">
            <span class="pill notice">update</span>
            <span class="grow">
              <span class="name">{update.label}</span>
              <span class="sub">{update.detail}</span>
            </span>
            <button class="button small" disabled={toolbox.preparing || toolbox.busy} onclick={() => updateFirmware(index, update.label)}>Update…</button>
          </div>
        {/each}
      </section>

      <section>
        <h2 class="section">Disks</h2>
        {#each found.disks as disk (disk.name)}
          <div class="line">
            <span class="mono disk">{disk.name}</span>
            <span class="grow">
              <span class="name">{disk.label}</span>
              <span class="sub">{disk.detail}</span>
            </span>
            <button class="button small" disabled={toolbox.preparing || toolbox.busy} title="Reads the disk's own health record; needs root" onclick={() => toolbox.reviewWith(() => api.prepareSmart(disk.name))}>
              Check health…
            </button>
          </div>
        {:else}
          <p class="muted">No disks found.</p>
        {/each}
      </section>

      <section>
        <h2 class="section">Temperatures</h2>
        {#if found.sensors.length === 0}
          <p class="muted">No temperature sensors found.</p>
        {:else}
          <div class="sensors">
            {#each found.sensors as sensor, index (index)}
              <div class="sensor" class:hot={sensor.attention}>
                <span class="sub">{sensor.name}</span>
                <span class="value">{sensor.label}</span>
                {#if sensor.detail}<span class="faint tiny">{sensor.detail}</span>{/if}
              </div>
            {/each}
          </div>
        {/if}
      </section>

      {#if found.batteries.length > 0}
        <section>
          <h2 class="section">Batteries</h2>
          {#each found.batteries as battery (battery.name)}
            <div class="line">
              <span class="mono disk">{battery.name}</span>
              <span class="grow">
                <span class="name">{battery.label}</span>
                <span class="sub">{battery.detail}</span>
              </span>
            </div>
          {/each}
        </section>
      {/if}

      <section>
        <h2 class="section">Devices</h2>
        <div class="tabs">
          <button class="button" class:primary={showDevices === 'pci'} onclick={() => (showDevices = showDevices === 'pci' ? 'none' : 'pci')}>PCI ({found.pci.length})</button>
          <button class="button" class:primary={showDevices === 'usb'} onclick={() => (showDevices = showDevices === 'usb' ? 'none' : 'usb')}>USB ({found.usb.length})</button>
        </div>
        {#if showDevices !== 'none'}
          <table>
            <tbody>
              {#each devices as device (device.address)}
                <tr>
                  <td class="mono faint">{device.address}</td>
                  <td>{device.name}</td>
                  <td class="muted">{device.class}</td>
                  <td class="mono muted">{device.driver}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </section>
    {/if}
  </div>
</div>

<style>
  .content {
    padding: 4px 18px 18px;
  }

  section + section {
    margin-top: 6px;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 10px;
    border-radius: var(--radius-small);
  }

  .line:hover {
    background: var(--panel-raised);
  }

  .grow {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  .name {
    font-weight: 600;
  }

  .sub {
    color: var(--muted);
    font-size: 13px;
  }

  .disk {
    min-width: 80px;
    color: var(--cyan);
  }

  .button.small {
    height: 28px;
    padding: 0 10px;
  }

  .sensors {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 8px;
  }

  .sensor {
    display: grid;
    padding: 8px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
  }

  .sensor.hot {
    border-color: var(--red);
  }

  .sensor.hot .value {
    color: var(--red);
  }

  .value {
    font-size: 18px;
    font-weight: 700;
  }

  .tiny {
    font-size: 11px;
  }

  .tabs {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  td {
    padding: 5px 8px;
    border-bottom: 1px solid var(--line);
  }

  .bad {
    color: var(--red);
  }
</style>
