<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, apiError } from '$lib/api';
  type Point = { x:number; y:number };
  type Zone = { id:string; name:string; colour:string; polygon_coordinates:Point[]; camera_id:string };
  type Camera = { id:string; name:string; status:string };
  let cameraId = ''; let name = 'Backyard'; let colour = '#74e0b4'; let points: Point[] = [];
  let zones: Zone[] = []; let cameras: Camera[] = []; let message = '';
  let canvas: HTMLCanvasElement;
  let previewTick = 0; let previewError = false; let livePreview = true;
  let timer: ReturnType<typeof setInterval> | undefined;
  let showOverlays = true;
  let mousePos: Point | null = null;

  $: previewSrc = cameraId ? `/api/cameras/${cameraId}/snapshot?t=${previewTick}` : '';
  $: cameraZones = zones.filter((z) => z.camera_id === cameraId);
  $: canClose = points.length >= 3;
  const cameraName = (id: string) => cameras.find((camera) => camera.id === id)?.name ?? id.slice(0, 8);

  function canvasCoords(event: MouseEvent): Point {
    const rect = canvas.getBoundingClientRect();
    return {
      x: Number(((event.clientX - rect.left) / rect.width).toFixed(4)),
      y: Number(((event.clientY - rect.top) / rect.height).toFixed(4))
    };
  }

  function draw(event: MouseEvent) {
    const pt = canvasCoords(event);
    // If 3+ points and click is near the first point, close the polygon
    if (canClose) {
      const first = points[0];
      const dx = (pt.x - first.x) * canvas.width;
      const dy = (pt.y - first.y) * canvas.height;
      if (Math.sqrt(dx * dx + dy * dy) < 14) {
        // Close polygon — save is triggered separately
        return;
      }
    }
    points = [...points, pt];
    render();
  }

  function onMouseMove(event: MouseEvent) {
    mousePos = canvasCoords(event);
    render();
  }

  function onMouseLeave() {
    mousePos = null;
    render();
  }

  function render() {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const w = canvas.width; const h = canvas.height;
    ctx.clearRect(0, 0, w, h);

    // Draw existing zones for this camera
    if (showOverlays && cameraId) {
      for (const zone of cameraZones) {
        if (zone.polygon_coordinates.length < 3) continue;
        ctx.beginPath();
        ctx.moveTo(zone.polygon_coordinates[0].x * w, zone.polygon_coordinates[0].y * h);
        for (let i = 1; i < zone.polygon_coordinates.length; i++) {
          ctx.lineTo(zone.polygon_coordinates[i].x * w, zone.polygon_coordinates[i].y * h);
        }
        ctx.closePath();
        ctx.fillStyle = hexToRgba(zone.colour, 0.18);
        ctx.fill();
        ctx.strokeStyle = hexToRgba(zone.colour, 0.8);
        ctx.lineWidth = 2;
        ctx.stroke();
        // Label
        const cx = zone.polygon_coordinates.reduce((s, p) => s + p.x, 0) / zone.polygon_coordinates.length;
        const cy = zone.polygon_coordinates.reduce((s, p) => s + p.y, 0) / zone.polygon_coordinates.length;
        ctx.font = '13px system-ui, sans-serif';
        ctx.fillStyle = zone.colour;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(zone.name, cx * w, cy * h);
      }
    }

    // Draw the polygon being created
    if (points.length > 0) {
      ctx.beginPath();
      ctx.moveTo(points[0].x * w, points[0].y * h);
      for (let i = 1; i < points.length; i++) {
        ctx.lineTo(points[i].x * w, points[i].y * h);
      }
      // Rubber-band line to cursor
      if (mousePos) {
        ctx.lineTo(mousePos.x * w, mousePos.y * h);
      }
      ctx.strokeStyle = colour;
      ctx.lineWidth = 2;
      ctx.setLineDash([]);
      ctx.stroke();

      // Fill the polygon preview
      if (mousePos && points.length >= 2) {
        ctx.beginPath();
        ctx.moveTo(points[0].x * w, points[0].y * h);
        for (let i = 1; i < points.length; i++) {
          ctx.lineTo(points[i].x * w, points[i].y * h);
        }
        ctx.lineTo(mousePos.x * w, mousePos.y * h);
        ctx.closePath();
        ctx.fillStyle = hexToRgba(colour, 0.12);
        ctx.fill();
      }

      // Draw closing line to first point when closeable
      if (canClose && mousePos) {
        const first = points[0];
        const dx = (mousePos.x - first.x) * w;
        const dy = (mousePos.y - first.y) * h;
        if (Math.sqrt(dx * dx + dy * dy) < 14) {
          // Snap indicator — highlight first point
          ctx.beginPath();
          ctx.arc(first.x * w, first.y * h, 10, 0, Math.PI * 2);
          ctx.fillStyle = hexToRgba(colour, 0.35);
          ctx.fill();
          ctx.strokeStyle = colour;
          ctx.lineWidth = 2;
          ctx.stroke();
        }
      }

      // Draw vertex dots
      for (let i = 0; i < points.length; i++) {
        ctx.beginPath();
        ctx.arc(points[i].x * w, points[i].y * h, 5, 0, Math.PI * 2);
        ctx.fillStyle = i === 0 && canClose ? colour : '#74e0b4';
        ctx.fill();
        ctx.strokeStyle = '#0b1017';
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }

      // First point label when closeable
      if (canClose) {
        ctx.font = 'bold 10px system-ui, sans-serif';
        ctx.fillStyle = '#0b1017';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('1', points[0].x * w, points[0].y * h);
      }
    }
  }

  function hexToRgba(hex: string, alpha: number): string {
    const h = hex.replace('#', '');
    const r = parseInt(h.substring(0, 2), 16);
    const g = parseInt(h.substring(2, 4), 16);
    const b = parseInt(h.substring(4, 6), 16);
    return `rgba(${r},${g},${b},${alpha})`;
  }

  function refreshPreview() { if (cameraId) previewTick = Date.now(); }

  async function loadCameras() { const response = await api('/api/cameras'); if (response.ok) cameras = await response.json(); }
  async function load() {
    const response = await api('/api/zones');
    if (response.ok) zones = await response.json();
    render();
  }

  async function save() {
    const response = await api('/api/zones', { method:'POST', json:{camera_id:cameraId,name,colour,polygon_coordinates:points,enabled:true} });
    message = response.ok ? 'Zone saved' : await apiError(response);
    if (response.ok) { points = []; await load(); }
  }
  async function remove(zone: Zone) {
    if (!confirm(`Delete zone "${zone.name}"?`)) return;
    const response = await api(`/api/zones/${zone.id}`, { method:'DELETE' });
    message = response.ok ? `${zone.name} deleted` : await apiError(response);
    if (response.ok) await load();
  }

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
      <select bind:value={cameraId} on:change={() => { points = []; refreshPreview(); render(); }}>
        <option value="">Select a camera…</option>
        {#each cameras as camera}<option value={camera.id}>{camera.name}{camera.status && camera.status !== 'online' ? ` · ${camera.status}` : ''}</option>{/each}
      </select>
      <input bind:value={name} placeholder="Zone name" />
      <input type="color" bind:value={colour} />
      <button on:click={save} disabled={points.length < 3 || !cameraId}>Save zone</button>
    </div>
    {#if cameras.length === 0}<p class="muted small">No cameras yet. Add one on the <a href="/cameras">Cameras</a> page first.</p>{/if}
    <div class="canvas-wrap">
      {#if cameraId}<img src={previewSrc} alt="Camera preview" on:load={() => { previewError = false; render(); }} on:error={() => previewError = true} />{/if}
      <canvas bind:this={canvas} width={960} height={540}
        on:click={draw}
        on:mousemove={onMouseMove}
        on:mouseleave={onMouseLeave}
      ></canvas>
      {#if cameraId && previewError}<div class="preview-error">Preview unavailable — the camera may be offline. You can still place points.</div>{:else if !cameraId}<div class="preview-error">Select a camera to load a preview.</div>{/if}
    </div>
    <div class="preview-bar">
      <span class="muted small">{points.length} points{canClose ? ' — click near point 1 to close' : ''}</span>
      {#if cameraZones.length > 0}
        <label class="live"><input type="checkbox" bind:checked={showOverlays} on:change={render} /> Show zones ({cameraZones.length})</label>
      {/if}
      <label class="live"><input type="checkbox" bind:checked={livePreview} /> Live preview</label>
      <button class="ghost" on:click={refreshPreview} disabled={!cameraId}>Refresh</button>
    </div>
    {#if message}<p class="pill">{message}</p>{/if}
  </section>
  <aside class="card"><h2>Saved zones</h2>{#if zones.length === 0}<p class="muted">No zones yet.</p>{:else}{#each zones as zone}<div class="zone-row"><span class="swatch" style={`background:${zone.colour}`}></span><div><strong>{zone.name}</strong><small>{cameraName(zone.camera_id)} · {zone.polygon_coordinates.length} vertices</small></div><button class="del" on:click={() => remove(zone)} title="Delete zone">✕</button></div>{/each}{/if}</aside>
</div>
<style>.editor{display:grid;grid-template-columns:1fr 280px;gap:20px;margin-top:32px}.toolbar{display:flex;gap:8px;flex-wrap:wrap;margin-bottom:14px}.toolbar select,.toolbar input:not([type=color]){flex:1;min-width:130px;background:#0b1017;border:1px solid #2a4050;border-radius:6px;color:#e7edf5;padding:10px}.toolbar button{background:#74e0b4;color:#0b1017;border:0;border-radius:6px;padding:0 14px;font-weight:700}.canvas-wrap{position:relative;aspect-ratio:16/9;background:#0b1017;overflow:hidden;border-radius:8px;border:1px solid #2a4050}.canvas-wrap img,.canvas-wrap canvas{position:absolute;inset:0;width:100%;height:100%;object-fit:cover}.canvas-wrap canvas{cursor:crosshair}.preview-error{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;text-align:center;padding:0 24px;color:#8293a4;font-size:.85rem;pointer-events:none}.preview-bar{display:flex;align-items:center;gap:14px;flex-wrap:wrap;margin-top:12px}.preview-bar .live{display:flex;align-items:center;gap:6px;color:#8293a4;font-size:.8rem}.preview-bar .ghost{border:1px solid #355164;background:transparent;color:#e7edf5;border-radius:6px;padding:6px 10px}.small{font-size:.8rem}.zone-row{display:flex;align-items:center;gap:10px;padding:12px 0;border-bottom:1px solid #223442}.zone-row>div{flex:1}.zone-row small{display:block;color:#8293a4;margin-top:4px}.zone-row .del{border:1px solid #6d2a35;background:transparent;color:#f0788a;border-radius:6px;padding:4px 8px;cursor:pointer}.swatch{width:12px;height:12px;border-radius:50%}a{color:#74e0b4}@media(max-width:800px){.editor{grid-template-columns:1fr}}</style>
