<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, apiError } from '$lib/api';
  type Point = { x:number; y:number };
  type Zone = { id:string; name:string; colour:string; polygon_coordinates:Point[]; camera_id:string };
  type Camera = { id:string; name:string; status:string };
  let cameraId = ''; let name = 'Backyard'; let colour = '#74e0b4'; let points: Point[] = []; let zones: Zone[] = []; let cameras: Camera[] = []; let message = '';
  let canvas: HTMLCanvasElement;
  let previewTick = 0; let previewError = false; let livePreview = true; let timer: ReturnType<typeof setInterval> | undefined;

  $: previewSrc = cameraId ? `/api/cameras/${cameraId}/snapshot?t=${previewTick}` : '';
  const cameraName = (id: string) => cameras.find((camera) => camera.id === id)?.name ?? id.slice(0, 8);

  function draw(event: MouseEvent) { const rect = canvas.getBoundingClientRect(); points = [...points, { x: Number(((event.clientX-rect.left)/rect.width).toFixed(4)), y: Number(((event.clientY-rect.top)/rect.height).toFixed(4)) }]; }
  function refreshPreview() { if (cameraId) previewTick = Date.now(); }

  async function loadCameras() { const response = await api('/api/cameras'); if (response.ok) cameras = await response.json(); }
  async function load() { const response = await api('/api/zones'); if (response.ok) zones = await response.json(); }

  async function save() { const response = await api('/api/zones', { method:'POST', json:{camera_id:cameraId,name,colour,polygon_coordinates:points,enabled:true} }); message = response.ok ? 'Zone saved' : await apiError(response); if (response.ok) { points=[]; await load(); } }
  async function remove(zone: Zone) { if (!confirm(`Delete zone "${zone.name}"?`)) return; const response = await api(`/api/zones/${zone.id}`, { method:'DELETE' }); message = response.ok ? `${zone.name} deleted` : await apiError(response); if (response.ok) await load(); }

  onMount(() => {
    loadCameras(); load();
    timer = setInterval(() => { if (livePreview) refreshPreview(); }, 5000);
  });
  onDestroy(() => { if (timer) clearInterval(timer); });
</script>
<svelte:head><title>Zones · Objexel</title></svelte:head>
<p class="eyebrow">Spatial intelligence</p><h1>Zone editor</h1><p class="muted">Draw normalized polygons over a camera view. Objects crossing them become spatial observations.</p>
<div class="editor">
  <section class="card">
    <div class="toolbar">
      <select bind:value={cameraId} on:change={refreshPreview}>
        <option value="">Select a camera…</option>
        {#each cameras as camera}<option value={camera.id}>{camera.name}{camera.status && camera.status !== 'online' ? ` · ${camera.status}` : ''}</option>{/each}
      </select>
      <input bind:value={name} placeholder="Zone name" />
      <input type="color" bind:value={colour} />
      <button on:click={save} disabled={points.length < 3 || !cameraId}>Save zone</button>
    </div>
    {#if cameras.length === 0}<p class="muted small">No cameras yet. Add one on the <a href="/cameras">Cameras</a> page first.</p>{/if}
    <div class="canvas-wrap">
      {#if cameraId}<img src={previewSrc} alt="Camera preview" on:load={() => previewError = false} on:error={() => previewError = true} />{/if}
      <canvas bind:this={canvas} on:click={draw}></canvas>
      {#if cameraId && previewError}<div class="preview-error">Preview unavailable — the camera may be offline, or its RTSP URL needs credentials. You can still place points.</div>{:else if !cameraId}<div class="preview-error">Select a camera to load a preview.</div>{/if}
      {#each points as point, index}<span class="point" style={`left:${point.x*100}%;top:${point.y*100}%`}>{index+1}</span>{/each}
    </div>
    <div class="preview-bar">
      <span class="muted small">{points.length} points · click to add polygon vertices · 0–1 normalized coordinates</span>
      <label class="live"><input type="checkbox" bind:checked={livePreview} /> Live preview (snapshot every 5s)</label>
      <button class="ghost" on:click={refreshPreview} disabled={!cameraId}>Refresh</button>
    </div>
    {#if message}<p class="pill">{message}</p>{/if}
  </section>
  <aside class="card"><h2>Saved zones</h2>{#if zones.length === 0}<p class="muted">No zones yet.</p>{:else}{#each zones as zone}<div class="zone-row"><span class="swatch" style={`background:${zone.colour}`}></span><div><strong>{zone.name}</strong><small>{cameraName(zone.camera_id)} · {zone.polygon_coordinates.length} vertices</small></div><button class="del" on:click={() => remove(zone)} title="Delete zone">✕</button></div>{/each}{/if}</aside>
</div>
<style>.editor{display:grid;grid-template-columns:1fr 280px;gap:20px;margin-top:32px}.toolbar{display:flex;gap:8px;flex-wrap:wrap;margin-bottom:14px}.toolbar select,.toolbar input:not([type=color]){flex:1;min-width:130px;background:#0b1017;border:1px solid #2a4050;border-radius:6px;color:#e7edf5;padding:10px}.toolbar button{background:#74e0b4;color:#0b1017;border:0;border-radius:6px;padding:0 14px;font-weight:700}.canvas-wrap{position:relative;aspect-ratio:16/9;background:#0b1017;overflow:hidden;border-radius:8px;border:1px solid #2a4050}.canvas-wrap img,.canvas-wrap canvas{position:absolute;inset:0;width:100%;height:100%;object-fit:cover}.canvas-wrap canvas{cursor:crosshair}.preview-error{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;text-align:center;padding:0 24px;color:#8293a4;font-size:.85rem;pointer-events:none}.point{position:absolute;transform:translate(-50%,-50%);background:#74e0b4;color:#0b1017;border-radius:50%;width:22px;height:22px;text-align:center;line-height:22px;font-size:11px;font-weight:800;pointer-events:none}.preview-bar{display:flex;align-items:center;gap:14px;flex-wrap:wrap;margin-top:12px}.preview-bar .live{display:flex;align-items:center;gap:6px;color:#8293a4;font-size:.8rem}.preview-bar .ghost{border:1px solid #355164;background:transparent;color:#dce8f1;border-radius:6px;padding:6px 10px}.small{font-size:.8rem}.zone-row{display:flex;align-items:center;gap:10px;padding:12px 0;border-bottom:1px solid #223342}.zone-row>div{flex:1}.zone-row small{display:block;color:#8293a4;margin-top:4px}.zone-row .del{border:1px solid #6d2a35;background:transparent;color:#f0788a;border-radius:6px;padding:4px 8px;cursor:pointer}.swatch{width:12px;height:12px;border-radius:50%}a{color:#74e0b4}@media(max-width:800px){.editor{grid-template-columns:1fr}}</style>
