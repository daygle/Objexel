<script lang="ts">
  import { onMount } from 'svelte';
  import { api, apiError } from '$lib/api';

  type Model = { id: string; name: string; version: string; default_model: boolean };
  type Camera = { id: string; name: string; rtsp_url: string; status: string; enabled: boolean; active_model_id?: string };
  type OnvifDevice = { endpoint: string; types: string[]; scopes: string[]; xaddrs: string[] };
  type OnvifProfile = { token: string; name?: string; width?: number; height?: number; rtsp_uri?: string };

  let cameras: Camera[] = [];
  let models: Model[] = [];
  let message = '';
  let onvifDevices: OnvifDevice[] = [];
  let discovering = false;
  let onvifUsername = ''; let onvifPassword = ''; let selectedService = ''; let onvifProfiles: OnvifProfile[] = []; let loadingProfiles = false;

  // New camera form
  let name = '';
  let rtspUrl = '';
  let enabled = true;
  let saving = false;
  let credUser = ''; let credPass = '';

  function buildRtsp(url: string, user: string, pass: string): string {
    const trimmed = url.trim();
    if (user && !trimmed.includes('@')) {
      return trimmed.replace(/^(rtsps?:\/\/)/i, `$1${encodeURIComponent(user)}:${encodeURIComponent(pass)}@`);
    }
    return trimmed;
  }
  // Mask the password portion of an rtsp URL for display: rtsp://user:••••@host/...
  function maskRtsp(url: string): string {
    return url.replace(/(rtsps?:\/\/[^:/@\s]+:)[^@/\s]+(@)/i, '$1••••$2');
  }

  // Inline edit state
  let editingId: string | null = null;
  let editName = '';
  let editRtsp = '';
  let editEnabled = true;

  async function load() {
    const [cameraResponse, modelResponse] = await Promise.all([api('/api/cameras'), api('/api/models')]);
    if (cameraResponse.ok) cameras = await cameraResponse.json();
    if (modelResponse.ok) models = await modelResponse.json();
  }

  async function create() {
    if (!name.trim() || !rtspUrl.trim()) { message = 'Name and RTSP URL are required'; return; }
    saving = true;
    const response = await api('/api/cameras', { method: 'POST', json: { name: name.trim(), rtsp_url: buildRtsp(rtspUrl, credUser, credPass), enabled } });
    message = response.ok ? `Added ${name.trim()}` : await apiError(response);
    if (response.ok) { name = ''; rtspUrl = ''; credUser = ''; credPass = ''; enabled = true; await load(); }
    saving = false;
  }

  function startEdit(camera: Camera) {
    editingId = camera.id; editName = camera.name; editRtsp = camera.rtsp_url; editEnabled = camera.enabled;
  }
  function cancelEdit() { editingId = null; }

  async function saveEdit(camera: Camera) {
    const response = await api(`/api/cameras/${camera.id}`, { method: 'PUT', json: { name: editName.trim(), rtsp_url: editRtsp.trim(), enabled: editEnabled } });
    message = response.ok ? `${editName.trim()} updated` : await apiError(response);
    if (response.ok) { editingId = null; await load(); }
  }

  async function remove(camera: Camera) {
    if (!confirm(`Delete camera "${camera.name}"? This cannot be undone.`)) return;
    const response = await api(`/api/cameras/${camera.id}`, { method: 'DELETE' });
    message = response.ok ? `${camera.name} deleted` : await apiError(response);
    if (response.ok) await load();
  }

  async function loadOnvifProfiles(serviceUrl: string) {
    selectedService = serviceUrl; loadingProfiles = true; onvifProfiles = [];
    const response = await api('/api/onvif/profiles', { method: 'POST', json: { device_service_url: serviceUrl, username: onvifUsername, password: onvifPassword } });
    if (response.ok) { onvifProfiles = (await response.json()).profiles ?? []; message = `Found ${onvifProfiles.length} ONVIF profile(s)`; }
    else message = await apiError(response);
    loadingProfiles = false;
  }

  async function discoverOnvif() {
    discovering = true;
    message = 'Searching the local network for ONVIF cameras…';
    const response = await api('/api/onvif/discover', { method: 'POST' });
    if (response.ok) {
      const result = await response.json();
      onvifDevices = result.devices ?? [];
      message = onvifDevices.length ? `Found ${onvifDevices.length} ONVIF device(s)` : 'No ONVIF devices found';
    } else message = await apiError(response);
    discovering = false;
  }

  async function test(camera: Camera) {
    message = `Testing ${camera.name}…`;
    const response = await api(`/api/cameras/${camera.id}/test`, { method: 'POST' });
    if (response.ok) {
      const result = await response.json();
      message = result.status === 'online'
        ? `${camera.name} is online${result.latency_ms != null ? ` · ${result.latency_ms} ms` : ''}`
        : `${camera.name} unreachable${result.error ? `: ${result.error}` : ''}`;
      await load();
    } else message = await apiError(response);
  }

  async function assign(camera: Camera, modelId: string) {
    const response = await api(`/api/cameras/${camera.id}/model/${modelId}`, { method: 'POST' });
    message = response.ok ? `${camera.name} is now using ${models.find((model) => model.id === modelId)?.name ?? 'the selected model'}` : await apiError(response);
    if (response.ok) await load();
  }

  function useRtsp(profile: OnvifProfile) {
    if (!profile.rtsp_uri) return;
    // Keep the URL credential-free and carry the ONVIF credentials in the masked
    // Username/Password fields; they are combined into the RTSP URL on save.
    rtspUrl = profile.rtsp_uri;
    credUser = onvifUsername; credPass = onvifPassword;
    message = 'RTSP filled below — credentials are in the Username/Password fields; review, then Add camera.';
  }

  onMount(load);
</script>

<svelte:head><title>Cameras · Objexel</title></svelte:head>
<p class="eyebrow">Inference routing</p>
<h1>Camera configuration</h1>
<p class="muted">Register RTSP cameras, verify connectivity, and choose the detector that best fits each stream. Cameras without an override use the active global model.</p>
{#if message}<p class="pill">{message}</p>{/if}

<section class="card form">
  <div class="row"><div><strong>ONVIF discovery</strong><p class="muted">Searches the local network. Discovery may not cross Docker, VLAN, or firewall boundaries.</p></div><button class="ghost" on:click={discoverOnvif} disabled={discovering}>{discovering ? 'Discovering…' : 'Discover ONVIF cameras'}</button></div>
  <div class="onvif-credentials"><input bind:value={onvifUsername} placeholder="ONVIF username" /><input type="password" bind:value={onvifPassword} placeholder="ONVIF password" /></div>
  {#if onvifDevices.length}<div class="discovered">{#each onvifDevices as device}<div class="discovered-row"><span><strong>{device.scopes.find((scope) => scope.includes('/name/'))?.split('/name/')[1] ?? 'ONVIF device'}</strong><small>{device.xaddrs[0] ?? device.endpoint}</small></span><button class="ghost" on:click={() => loadOnvifProfiles(device.xaddrs[0])} disabled={!device.xaddrs[0] || loadingProfiles}>{selectedService === device.xaddrs[0] && loadingProfiles ? 'Loading…' : 'Get profiles'}</button></div>{/each}</div>{/if}
  {#if onvifProfiles.length}<div class="profiles-list">{#each onvifProfiles as profile}<div class="discovered-row"><span><strong>{profile.name ?? profile.token}</strong><small>{profile.width ?? '?'} × {profile.height ?? '?'}{profile.rtsp_uri ? ` · ${profile.rtsp_uri}` : ' · no RTSP URI'}</small></span><button class="ghost" on:click={() => useRtsp(profile)}>Use RTSP</button></div>{/each}</div>{/if}
</section>

<section class="card form">
  <div class="form-row">
    <input bind:value={name} placeholder="Camera name" />
    <input bind:value={rtspUrl} placeholder="rtsp://host:554/stream" />
    <input bind:value={credUser} placeholder="Username (optional)" autocomplete="off" />
    <input type="password" bind:value={credPass} placeholder="Password (optional)" autocomplete="new-password" />
    <label class="check"><input type="checkbox" bind:checked={enabled} /> Enabled</label>
    <button on:click={create} disabled={saving || !name.trim() || !rtspUrl.trim()}>{saving ? 'Adding…' : 'Add camera'}</button>
  </div>
</section>

{#if cameras.length === 0}
  <div class="empty">No cameras configured yet. Add one above to begin ingestion.</div>
{:else}
  <div class="card table-wrap">
    <table class="table">
      <thead><tr><th>Camera</th><th>Stream</th><th>Status</th><th>Inference model</th><th>Actions</th></tr></thead>
      <tbody>
        {#each cameras as camera}
          <tr>
            {#if editingId === camera.id}
              <td><input bind:value={editName} /></td>
              <td><input class="wide" bind:value={editRtsp} /></td>
              <td><label class="check"><input type="checkbox" bind:checked={editEnabled} /> Enabled</label></td>
              <td class="muted">—</td>
              <td class="actions"><button on:click={() => saveEdit(camera)}>Save</button><button class="ghost" on:click={cancelEdit}>Cancel</button></td>
            {:else}
              <td><strong>{camera.name}</strong><small>{camera.id.slice(0, 8)}{camera.enabled ? '' : ' · disabled'}</small></td>
              <td class="muted">{maskRtsp(camera.rtsp_url)}</td>
              <td><span class:online={camera.status === 'online'} class="status">{camera.status}</span></td>
              <td>
                <select value={camera.active_model_id ?? ''} on:change={(event) => assign(camera, (event.currentTarget as HTMLSelectElement).value)}>
                  <option value="">Global default</option>
                  {#each models as model}
                    <option value={model.id}>{model.name} · {model.version}{model.default_model ? ' · default' : ''}</option>
                  {/each}
                </select>
              </td>
              <td class="actions">
                <button on:click={() => test(camera)}>Test</button>
                <button class="ghost" on:click={() => startEdit(camera)}>Edit</button>
                <button class="danger" on:click={() => remove(camera)}>Delete</button>
              </td>
            {/if}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .form { margin-top: 28px; }
  .form .row{align-items:flex-start}.form .row>div{flex:1}.form .row p{margin:5px 0 0}.form button.ghost{border:1px solid #355164;border-radius:6px;background:transparent;color:#dce8f1;padding:9px 12px}  .onvif-credentials{display:flex;gap:8px;flex-wrap:wrap}.onvif-credentials input{flex:1;min-width:180px;background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:9px}.discovered,.profiles-list{display:grid;gap:8px;margin-top:12px;padding-top:12px;border-top:1px solid #223342}.discovered-row{display:flex;justify-content:space-between;align-items:center;gap:12px;padding:9px;background:#0f1922;border-radius:6px}.discovered-row small{display:block;word-break:break-all}

  .form-row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .form-row input:not([type=checkbox]) { flex: 1; min-width: 180px; background: #0b1017; color: #e7edf5; border: 1px solid #355164; border-radius: 6px; padding: 10px; }
  .check { display: flex; align-items: center; gap: 6px; color: #8293a4; font-size: .85rem; }
  .form-row button { border: 0; border-radius: 6px; padding: 10px 14px; background: #74e0b4; color: #0b1017; font-weight: 700; }
  .table-wrap { margin-top: 24px; overflow-x: auto; }
  small { display: block; color: #71869b; margin-top: 5px; }
  input.wide { min-width: 240px; }
  td input:not([type=checkbox]) { background: #0b1017; color: #e7edf5; border: 1px solid #355164; border-radius: 6px; padding: 8px; }
  select { min-width: 200px; background: #0b1017; color: #e7edf5; border: 1px solid #355164; border-radius: 6px; padding: 9px; }
  .status { padding: 5px 9px; border-radius: 99px; background: #3d2630; color: #f0788a; font-size: .75rem; }
  .status.online { background: #18352f; color: #74e0b4; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .actions button { border: 1px solid #355164; border-radius: 6px; background: #162532; color: #dce8f1; padding: 7px 10px; font-size: .8rem; }
  .actions button.ghost { background: transparent; }
  .actions button.danger { border-color: #6d2a35; color: #f0788a; }
  @media (max-width: 800px) { select { min-width: 150px; } }
</style>
