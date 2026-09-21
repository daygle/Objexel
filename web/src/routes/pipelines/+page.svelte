<script lang="ts">
  import { onMount } from 'svelte';
  import { api, apiError } from '$lib/api';
  type Assignment = { id:string; camera_id:string; model_id:string; priority:number; confidence_threshold:number; fps_limit?:number; enabled:boolean };
  type Fusion = { id:string; camera_id:string; detection_id:string; source_model_ids:string[]; fused_confidence:number; created_at:string };
  type Camera = { id:string; name:string }; type Model = { id:string; name:string; version:string };
  let assignments: Assignment[] = []; let fusion: Fusion[] = []; let cameras: Camera[] = []; let models: Model[] = []; let message = '';

  // Editor state
  let editingId: string | null = null;
  let editCamera = ''; let editModel = ''; let editPriority = 0; let editThreshold = 25; let editFps = ''; let editEnabled = true;

  async function load() {
    const responses = await Promise.all([api('/api/model-assignments'), api('/api/fusion?limit=50'), api('/api/cameras'), api('/api/models')]);
    if (responses[0].ok) assignments = await responses[0].json();
    if (responses[1].ok) fusion = await responses[1].json();
    if (responses[2].ok) cameras = await responses[2].json();
    if (responses[3].ok) models = await responses[3].json();
  }

  async function addAssignment() {
    const camera_id = cameras[0]?.id; const model_id = models[0]?.id;
    if (!camera_id || !model_id) { message = 'Add a camera and model first'; return; }
    const response = await api('/api/model-assignments', { method:'POST', json:{ camera_id, model_id, priority: assignments.length, confidence_threshold:.25, enabled:true } });
    message = response.ok ? 'Model assignment saved' : await apiError(response);
    await load();
  }

  function startEdit(item: Assignment) {
    editingId = item.id;
    editCamera = item.camera_id; editModel = item.model_id;
    editPriority = item.priority; editThreshold = Math.round(item.confidence_threshold * 100);
    editFps = item.fps_limit != null ? String(item.fps_limit) : ''; editEnabled = item.enabled;
  }

  function cancelEdit() { editingId = null; }

  async function saveEdit() {
    if (!editingId) return;
    const payload = {
      priority: editPriority,
      confidence_threshold: Math.min(1, Math.max(0, editThreshold / 100)),
      fps_limit: editFps.trim() ? Number(editFps) : null,
      enabled: editEnabled
    };
    const response = await api(`/api/model-assignments/${editingId}`, { method:'PUT', json: payload });
    message = response.ok ? 'Assignment updated' : await apiError(response);
    if (response.ok) { editingId = null; await load(); }
  }

  async function remove(item: Assignment) {
    if (!confirm('Delete this model assignment?')) return;
    const response = await api(`/api/model-assignments/${item.id}`, { method:'DELETE' });
    message = response.ok ? 'Assignment deleted' : await apiError(response);
    if (response.ok) await load();
  }

  onMount(load);
</script>

<svelte:head><title>AI Pipelines · Objexel</title></svelte:head>
<div class="row"><div><p class="eyebrow">Multi-model inference</p><h1>AI Pipelines</h1><p class="muted">Run specialized models against the same stream, then fuse overlapping detections before tracking.</p></div><button on:click={addAssignment}>Add assignment</button></div>
{#if message}<p class="pill">{message}</p>{/if}
<div class="metrics"><div class="card"><span class="muted">Active assignments</span><div class="metric">{assignments.filter((item) => item.enabled).length}</div></div><div class="card"><span class="muted">Fused detections</span><div class="metric">{fusion.length}</div></div><div class="card"><span class="muted">Models available</span><div class="metric">{models.length}</div></div></div>

<section class="card">
  <div class="row"><h2>Model assignments</h2><span class="muted">Higher priority wins close conflicts</span></div>
  {#if assignments.length === 0}
    <div class="empty">No per-camera assignments yet. The default model remains active.</div>
  {:else}
    <table class="table">
      <thead><tr><th>Camera</th><th>Model</th><th>Priority</th><th>Threshold</th><th>FPS limit</th><th>Status</th><th></th></tr></thead>
      <tbody>
        {#each assignments as item}
          {#if editingId === item.id}
            <tr>
              <td><select bind:value={editCamera} disabled>{#each cameras as c}<option value={c.id}>{c.name}</option>{/each}</select></td>
              <td><select bind:value={editModel} disabled>{#each models as m}<option value={m.id}>{m.name}</option>{/each}</select></td>
              <td><input type="number" bind:value={editPriority} min="0" /></td>
              <td><input type="number" bind:value={editThreshold} min="0" max="100" />%</td>
              <td><input type="number" bind:value={editFps} placeholder="—" min="0" step="0.1" /></td>
              <td><label class="check"><input type="checkbox" bind:checked={editEnabled} /> Enabled</label></td>
              <td class="actions"><button on:click={saveEdit}>Save</button><button class="ghost" on:click={cancelEdit}>Cancel</button></td>
            </tr>
          {:else}
            <tr>
              <td>{cameras.find((camera) => camera.id === item.camera_id)?.name ?? item.camera_id.slice(0,8)}</td>
              <td>{models.find((model) => model.id === item.model_id)?.name ?? item.model_id.slice(0,8)}</td>
              <td>{item.priority}</td>
              <td>{Math.round(item.confidence_threshold * 100)}%</td>
              <td>{item.fps_limit ?? '—'}</td>
              <td><span class="pill">{item.enabled ? 'enabled' : 'paused'}</span></td>
              <td class="actions">
                <button class="ghost" on:click={() => startEdit(item)}>Edit</button>
                <button class="danger" on:click={() => remove(item)}>Delete</button>
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<section class="card history">
  <div class="row"><h2>Fusion history</h2><span class="muted">Newest fused outputs</span></div>
  {#if fusion.length === 0}
    <div class="empty">Fusion results will appear once assigned models process frames.</div>
  {:else}
    {#each fusion as result}
      <div class="fusion-row"><span class="pill">{Math.round(result.fused_confidence * 100)}% fused</span><span>{result.source_model_ids.length} source models</span><span class="muted">{new Date(result.created_at).toLocaleString()}</span></div>
    {/each}
  {/if}
</section>

<style>
  h2{margin:0}.metrics{display:grid;grid-template-columns:repeat(3,1fr);gap:16px;margin:30px 0}.metric{font-size:2rem;margin-top:8px}.row button{background:#74e0b4;border:0;border-radius:6px;padding:10px 14px;color:#0b1017;font-weight:700}.history{margin-top:18px}.fusion-row{display:flex;gap:18px;align-items:center;border-top:1px solid #223442;padding:14px 0}.fusion-row span:nth-child(2){flex:1;color:#b9c8d5}
  .actions{display:flex;gap:6px}.actions button{border:1px solid #355164;border-radius:6px;background:#162532;color:#dce8f1;padding:6px 10px;font-size:.8rem;cursor:pointer}.actions button.ghost{background:transparent}.actions button.danger{border-color:#6d2a35;color:#f0788a}
  td input, td select{background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:6px 8px;font-size:.85rem}td input[type=number]{width:70px}
  .check{display:flex;align-items:center;gap:6px;color:#8293a4;font-size:.85rem}
  @media(max-width:700px){.metrics{grid-template-columns:1fr}.row{align-items:flex-start;flex-direction:column}.fusion-row{align-items:flex-start;flex-direction:column}}
</style>
