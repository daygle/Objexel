<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  type Behaviour = { id:string; track_id:string; camera_id:string; object_class:string; behaviour_type:string; confidence:number; summary:string; start_time:string; end_time:string; recording_id?:string; clip_id?:string };
  let behaviours: Behaviour[] = [];
  let filter = '';
  onMount(async () => { const response = await api('/api/behaviours?limit=100'); if (response.ok) behaviours = await response.json(); });
  $: filtered = behaviours.filter((item) => !filter || item.behaviour_type.includes(filter) || item.object_class.toLowerCase().includes(filter.toLowerCase()));
</script>
<svelte:head><title>Behaviours · Objexel</title></svelte:head>
<div class="row"><div><p class="eyebrow">Behavioural intelligence</p><h1>Behaviours</h1><p class="muted">Movement, dwell, and routes distilled from persistent object identities.</p></div><span class="pill">{filtered.length} classified</span></div>
<div class="toolbar"><input bind:value={filter} placeholder="Filter loitering, stationary, cat…" /><span class="muted">Rules can match behaviour types before creating events.</span></div>
{#if filtered.length === 0}<div class="empty">No behaviours yet. Keep a camera pipeline running until tracks accumulate enough movement history.</div>{:else}<div class="grid cards">{#each filtered as item}<article class="card"><div class="row"><span class="pill">{item.behaviour_type.replaceAll('_',' ')}</span><strong>{Math.round(item.confidence * 100)}%</strong></div><h2>{item.object_class}</h2><p>{item.summary}</p><div class="meta"><span>{Math.round((new Date(item.end_time).getTime() - new Date(item.start_time).getTime()) / 1000)}s duration</span><span>{new Date(item.end_time).toLocaleString()}</span></div>{#if item.clip_id}<a href="/recordings">Open associated clip →</a>{/if}</article>{/each}</div>{/if}
<style>
  h2{margin:20px 0 6px;text-transform:capitalize}.toolbar{display:flex;align-items:center;gap:16px;margin:28px 0}.toolbar input{background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:11px;min-width:280px}.cards{grid-template-columns:repeat(auto-fit,minmax(280px,1fr));margin-top:18px}.card p{color:#a7b6c4;min-height:42px}.meta{display:flex;justify-content:space-between;border-top:1px solid #223342;padding-top:14px;color:#71869b;font-size:.78rem}.card a{display:block;color:#74e0b4;text-decoration:none;margin-top:18px}@media(max-width:650px){.toolbar{align-items:flex-start;flex-direction:column}.toolbar input{width:100%;min-width:0}}
</style>
