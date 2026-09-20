<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  type Observation = { id:string; observation_type:string; summary:string; track_id:string; created_at:string };
  let observations: Observation[] = [];
  let loading = true;
  onMount(async () => { try { const response = await api('/api/observations?limit=50'); if (response.ok) observations = await response.json(); } finally { loading = false; } });
</script>
<svelte:head><title>Observations · Objexel</title></svelte:head>
<p class="eyebrow">Behavioral intelligence</p><h1>Observations</h1><p class="muted">Raw detections become useful answers here.</p>
<div class="grid" style="grid-template-columns:repeat(3,1fr);margin:32px 0"><div class="card"><span class="muted">Recent observations</span><div class="metric">{observations.length}</div></div><div class="card"><span class="muted">Pipeline</span><div class="metric">Live</div></div><div class="card"><span class="muted">Storage</span><div class="metric">Postgres</div></div></div>
{#if loading}<div class="empty">Loading observations…</div>{:else if observations.length === 0}<div class="empty">No observations yet. Start a camera pipeline to populate this view.</div>{:else}<div class="card"><table class="table"><thead><tr><th>Type</th><th>Summary</th><th>Track</th><th>Created</th></tr></thead><tbody>{#each observations as observation}<tr><td><span class="pill">{observation.observation_type}</span></td><td>{observation.summary}</td><td class="muted">{observation.track_id.slice(0,8)}</td><td class="muted">{new Date(observation.created_at).toLocaleString()}</td></tr>{/each}</tbody></table></div>{/if}
