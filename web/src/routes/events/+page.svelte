<script lang="ts">
  import { onMount } from 'svelte';
  type Event = { id:string; summary:string; event_type:string; severity:string; rule_id:string; created_at:string };
  let events: Event[] = []; let loading = true;
  onMount(async () => { try { const response = await fetch('/api/events?limit=100'); if (response.ok) events = await response.json(); } finally { loading = false; } });
</script>
<svelte:head><title>Events · Objexel</title></svelte:head>
<p class="eyebrow">Rule output</p><h1>Events</h1><p class="muted">The final intelligence layer generated from observations.</p>
{#if loading}<div class="empty">Loading events…</div>{:else if events.length === 0}<div class="empty">No events generated yet.</div>{:else}<div class="card" style="margin-top:32px"><table class="table"><thead><tr><th>Severity</th><th>Summary</th><th>Type</th><th>Rule</th><th>Created</th></tr></thead><tbody>{#each events as event}<tr><td><span class:critical={event.severity === 'critical'} class:warning={event.severity === 'warning'} class="severity">{event.severity}</span></td><td><strong>{event.summary}</strong></td><td class="muted">{event.event_type}</td><td class="muted">{event.rule_id.slice(0,8)}</td><td class="muted">{new Date(event.created_at).toLocaleString()}</td></tr>{/each}</tbody></table></div>{/if}
<style>.severity{padding:5px 9px;border-radius:99px;background:#18352f;color:#74e0b4;font-size:.75rem}.severity.warning{background:#40351b;color:#f0c56d}.severity.critical{background:#401f29;color:#f0788a}</style>
