<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { subscribeLiveEvents } from '$lib/events';
  type Event = { id:string; summary:string; event_type:string; severity:string; rule_id:string; created_at:string };
  let events: Event[] = []; let loading = true;
  let live = false;
  let freshIds = new Set<string>();

  function prepend(event: Event) {
    if (!event?.id || events.some((item) => item.id === event.id)) return;
    freshIds = new Set(freshIds).add(event.id);
    events = [event, ...events].slice(0, 200);
    setTimeout(() => { freshIds = new Set([...freshIds].filter((id) => id !== event.id)); }, 4000);
  }

  onMount(() => {
    (async () => { try { const response = await api('/api/events?limit=100'); if (response.ok) events = await response.json(); } finally { loading = false; } })();
    return subscribeLiveEvents({
      onStatus: (connected) => { live = connected; },
      onMessage: (message) => { if (message.kind === 'event') prepend(message.data as Event); }
    });
  });
</script>
<svelte:head><title>Events · Objexel</title></svelte:head>
<div class="row"><div><p class="eyebrow">Rule output</p><h1>Events</h1><p class="muted">The final intelligence layer generated from observations.</p></div><span class="live" class:on={live}><i></i>{live ? 'Live' : 'Offline'}</span></div>
{#if loading}<div class="empty">Loading events…</div>{:else if events.length === 0}<div class="empty">No events generated yet. New events will appear here live as rules fire.</div>{:else}<div class="card" style="margin-top:32px"><table class="table"><thead><tr><th>Severity</th><th>Summary</th><th>Type</th><th>Rule</th><th>Created</th></tr></thead><tbody>{#each events as event (event.id)}<tr class:fresh={freshIds.has(event.id)}><td><span class:critical={event.severity === 'critical'} class:warning={event.severity === 'warning'} class="severity">{event.severity}</span></td><td><strong>{event.summary}</strong></td><td class="muted">{event.event_type}</td><td class="muted">{event.rule_id.slice(0,8)}</td><td class="muted">{new Date(event.created_at).toLocaleString()}</td></tr>{/each}</tbody></table></div>{/if}
<style>
  .severity{padding:5px 9px;border-radius:99px;background:#18352f;color:#74e0b4;font-size:.75rem}.severity.warning{background:#40351b;color:#f0c56d}.severity.critical{background:#401f29;color:#f0788a}
  .live{display:inline-flex;align-items:center;gap:7px;font-size:.78rem;color:#8293a4;border:1px solid #294050;border-radius:99px;padding:6px 12px;white-space:nowrap}
  .live i{width:8px;height:8px;border-radius:50%;background:#4a5b6b}
  .live.on{color:#74e0b4;border-color:#28624f}
  .live.on i{background:#74e0b4;box-shadow:0 0 10px #74e0b4;animation:pulse 1.6s ease-in-out infinite}
  @keyframes pulse{0%,100%{opacity:1}50%{opacity:.35}}
  tr.fresh td{animation:flash 4s ease-out}
  @keyframes flash{0%{background:#14352b}100%{background:transparent}}
</style>
