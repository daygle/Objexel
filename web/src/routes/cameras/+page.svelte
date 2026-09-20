<script lang="ts">
  import { onMount } from 'svelte';

  type Model = { id: string; name: string; version: string; default_model: boolean };
  type Camera = { id: string; name: string; rtsp_url: string; status: string; active_model_id?: string };

  let cameras: Camera[] = [];
  let models: Model[] = [];
  let message = '';

  async function load() {
    const [cameraResponse, modelResponse] = await Promise.all([fetch('/api/cameras'), fetch('/api/models')]);
    if (cameraResponse.ok) cameras = await cameraResponse.json();
    if (modelResponse.ok) models = await modelResponse.json();
  }

  async function assign(camera: Camera, modelId: string) {
    if (!modelId) return;
    const response = await fetch(`/api/cameras/${camera.id}/model/${modelId}`, { method: 'POST' });
    message = response.ok ? `${camera.name} is now using ${models.find((model) => model.id === modelId)?.name ?? 'the selected model'}` : await response.text();
    if (response.ok) await load();
  }

  onMount(load);
</script>

<svelte:head><title>Cameras · Objexel</title></svelte:head>
<p class="eyebrow">Inference routing</p>
<h1>Camera configuration</h1>
<p class="muted">Choose the detector that best fits each camera. Cameras without an override use the active global model.</p>
{#if message}<p class="pill">{message}</p>{/if}

{#if cameras.length === 0}
  <div class="empty">No cameras configured yet.</div>
{:else}
  <div class="card table-wrap">
    <table class="table">
      <thead><tr><th>Camera</th><th>Stream</th><th>Status</th><th>Inference model</th></tr></thead>
      <tbody>
        {#each cameras as camera}
          <tr>
            <td><strong>{camera.name}</strong><small>{camera.id.slice(0, 8)}</small></td>
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
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .table-wrap { margin-top: 32px; overflow-x: auto; }
  small { display: block; color: #71869b; margin-top: 5px; }
  select { min-width: 220px; background: #0b1017; color: #e7edf5; border: 1px solid #355164; border-radius: 6px; padding: 9px; }
  .status { padding: 5px 9px; border-radius: 99px; background: #3d2630; color: #f0788a; font-size: .75rem; }
  .status.online { background: #18352f; color: #74e0b4; }
  @media (max-width: 800px) { select { min-width: 160px; } }
</style>
