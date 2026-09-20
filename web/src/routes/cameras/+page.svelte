<script lang="ts">
  import { onMount } from 'svelte';
  import { api, apiError } from '$lib/api';

  type Model = { id: string; name: string; version: string; default_model: boolean };
  type Camera = { id: string; name: string; rtsp_url: string; status: string; enabled: boolean; active_model_id?: string };

  let cameras: Camera[] = [];
  let models: Model[] = [];
  let message = '';

  // New camera form
  let name = '';
  let rtspUrl = '';
  let enabled = true;
  let saving = false;

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
    const response = await api('/api/cameras', { method: 'POST', json: { name: name.trim(), rtsp_url: rtspUrl.trim(), enabled } });
    message = response.ok ? `Added ${name.trim()}` : await apiError(response);
    if (response.ok) { name = ''; rtspUrl = ''; enabled = true; await load(); }
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

  onMount(load);
</script>

<svelte:head><title>Cameras · Objexel</title></svelte:head>
<p class="eyebrow">Inference routing</p>
<h1>Camera configuration</h1>
<p class="muted">Register RTSP cameras, verify connectivity, and choose the detector that best fits each stream. Cameras without an override use the active global model.</p>
{#if message}<p class="pill">{message}</p>{/if}

<section class="card form">
  <div class="form-row">
    <input bind:value={name} placeholder="Camera name" />
    <input bind:value={rtspUrl} placeholder="rtsp://user:pass@host:554/stream" />
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
              <td class="muted">{camera.rtsp_url}</td>
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
