<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { subscribeLiveEvents } from '$lib/events';
  type Observation = { id:string; observation_type:string; summary:string; track_id:string; created_at:string };
  let observations: Observation[] = [];
  let loading = true;
  let live = false;
  let freshIds = new Set<string>();

  function prepend(observation: Observation) {
    if (!observation?.id || observations.some((item) => item.id === observation.id)) return;
    freshIds = new Set(freshIds).add(observation.id);
    observations = [observation, ...observations].slice(0, 100);
    setTimeout(() => { freshIds = new Set([...freshIds].filter((id) => id !== observation.id)); }, 4000);
  }

  onMount(() => {
    (async () => { try { const response = await api('/api/observations?limit=50'); if (response.ok) observations = await response.json(); } finally { loading = false; } })();
    return subscribeLiveEvents({
      onStatus: (connected) => { live = connected; },
      onMessage: (message) => { if (message.kind === 'observation') prepend(message.data as Observation); }
    });
  });
</script>
<svelte:head><title>Observations · Objexel</title></svelte:head>
<div class="row"><div><p class="eyebrow">Behavioral intelligence</p><h1>Observations</h1><p class="muted">Raw detections become useful answers here.</p></div><span class="live" class:on={live}><i></i>{live ? 'Live' : 'Offline'}</span></div>
<div class="grid" style="grid-template-columns:repeat(3,1fr);margin:32px 0"><div class="card"><span class="muted">Recent observations</span><div class="metric">{observations.length}</div></div><div class="card"><span class="muted">Pipeline</span><div class="metric">{live ? 'Streaming' : 'Idle'}</div></div><div class="card"><span class="muted">Storage</span><div class="metric">Postgres</div></div></div>
{#if loading}<div class="empty">Loading observations…</div>{:else if observations.length === 0}<div class="empty">No observations yet. Start a camera pipeline to populate this view.</div>{:else}<div class="card"><table class="table"><thead><tr><th>Type</th><th>Summary</th><th>Track</th><th>Created</th></tr></thead><tbody>{#each observations as observation (observation.id)}<tr class:fresh={freshIds.has(observation.id)}><td><span class="pill">{observation.observation_type}</span></td><td>{observation.summary}</td><td class="muted">{observation.track_id.slice(0,8)}</td><td class="muted">{new Date(observation.created_at).toLocaleString()}</td></tr>{/each}</tbody></table></div>{/if}
<style>
  .live{display:inline-flex;align-items:center;gap:7px;font-size:.78rem;color:#8293a4;border:1px solid #294050;border-radius:99px;padding:6px 12px;white-space:nowrap}
  .live i{width:8px;height:8px;border-radius:50%;background:#4a5b6b}
  .live.on{color:#74e0b4;border-color:#28624f}
  .live.on i{background:#74e0b4;box-shadow:0 0 10px #74e0b4;animation:pulse 1.6s ease-in-out infinite}
  @keyframes pulse{0%,100%{opacity:1}50%{opacity:.35}}
  tr.fresh td{animation:flash 4s ease-out}
  @keyframes flash{0%{background:#14352b}100%{background:transparent}}
</style>
