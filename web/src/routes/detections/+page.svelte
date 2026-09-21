<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  type Detection = { id:string; object_class:string; confidence:number; bounding_box:{x:number;y:number;width:number;height:number}; observed_at:string };
  let detections: Detection[] = [];
  let classFilter = '';
  $: classes = [...new Set(detections.map((d) => d.object_class))].sort();
  $: filtered = classFilter ? detections.filter((d) => d.object_class === classFilter) : detections;
  onMount(async () => { const response = await api('/api/detections?limit=100'); if (response.ok) detections = await response.json(); });
</script>

<p class="eyebrow">Model output</p>
<h1>Detections</h1>
<p class="muted">Confidence-scored objects entering the tracking pipeline.</p>

{#if detections.length === 0}
  <div class="empty">No detections available.</div>
{:else}
  <div class="toolbar">
    <select bind:value={classFilter}>
      <option value="">All classes ({detections.length})</option>
      {#each classes as cls}
        <option value={cls}>{cls} ({detections.filter((d) => d.object_class === cls).length})</option>
      {/each}
    </select>
  </div>
  <div class="card" style="margin-top:16px">
    <table class="table">
      <thead><tr><th>Object</th><th>Confidence</th><th>Bounding box</th><th>Observed</th></tr></thead>
      <tbody>
        {#each filtered as detection}
          <tr>
            <td><strong>{detection.object_class}</strong></td>
            <td><span class="pill">{Math.round(detection.confidence*100)}%</span></td>
            <td class="muted">{detection.bounding_box.x.toFixed(3)}, {detection.bounding_box.y.toFixed(3)} · {detection.bounding_box.width.toFixed(3)} × {detection.bounding_box.height.toFixed(3)}</td>
            <td class="muted">{new Date(detection.observed_at).toLocaleString()}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .toolbar { display: flex; gap: 8px; align-items: center; margin-top: 20px; }
  select { background: #0b1017; color: #e7edf5; border: 1px solid #355164; border-radius: 6px; padding: 9px 12px; }
</style>
