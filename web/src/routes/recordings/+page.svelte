<script lang="ts">
  import { onMount } from 'svelte';
  type Clip = { id:string; event_id?:string; clip_start:string; clip_end:string; clip_path:string };
  type Snapshot = { id:string; event_id?:string; camera_id:string; timestamp:string };
  type Recording = { id:string; camera_id:string; start_time:string; end_time?:string; file_path:string; mode:string };
  let clips: Clip[] = []; let snapshots: Snapshot[] = []; let recordings: Recording[] = []; let selected: Clip | null = null;
  onMount(async () => {
    const [clipResponse, snapshotResponse, recordingResponse] = await Promise.all([fetch('/api/clips?limit=100'), fetch('/api/snapshots?limit=100'), fetch('/api/recordings?limit=100')]);
    if (clipResponse.ok) clips = await clipResponse.json(); if (snapshotResponse.ok) snapshots = await snapshotResponse.json(); if (recordingResponse.ok) recordings = await recordingResponse.json();
  });
  async function download(clip: Clip) { const response = await fetch(`/api/clips/${clip.id}/download`, { method:'POST' }); if (!response.ok) return; const blob = await response.blob(); const url = URL.createObjectURL(blob); const anchor = document.createElement('a'); anchor.href = url; anchor.download = `${clip.id}.mp4`; anchor.click(); URL.revokeObjectURL(url); }
</script>
<svelte:head><title>Recordings · Objexel</title></svelte:head>
<div class="row"><div><p class="eyebrow">Video memory</p><h1>Recordings</h1><p class="muted">Continuous segments, event clips, and snapshots in one searchable timeline.</p></div><span class="pill">{clips.length} event clips</span></div>
<div class="layout">
  <section><div class="card"><div class="row"><h2>Event timeline</h2><span class="muted">Newest first</span></div>{#if clips.length === 0}<div class="empty">No event clips yet. Generated clips will appear when rules fire.</div>{:else}{#each clips as clip}<article class:selected={selected?.id === clip.id} class="timeline-item" on:click={() => selected = clip}><div class="dot"></div><div><strong>{new Date(clip.clip_start).toLocaleString()}</strong><p class="muted">{new Date(clip.clip_start).toLocaleTimeString()} → {new Date(clip.clip_end).toLocaleTimeString()}</p></div><button on:click|stopPropagation={() => download(clip)}>Download</button></article>{/each}{/if}</div></section>
  <aside>{#if selected}<div class="card player"><h2>Playback</h2><video controls autoplay src={`/api/clips/${selected.id}/media`}></video><p class="muted">Event clip · {new Date(selected.clip_start).toLocaleString()}</p></div>{:else}<div class="card empty">Select a timeline clip to preview it here.</div>{/if}<div class="card"><h2>Storage</h2><div class="metric">{recordings.length}</div><p class="muted">Continuous recording sessions</p><div class="snapshot-strip">{#each snapshots.slice(0,8) as snapshot}<img src={`/api/snapshots/${snapshot.id}/media`} alt="Event snapshot" loading="lazy" />{/each}</div></div></aside>
</div>
<style>
  h2{margin:0}.layout{display:grid;grid-template-columns:minmax(0,1.4fr) minmax(280px,.8fr);gap:20px;margin-top:32px}.timeline-item{display:flex;align-items:center;gap:14px;padding:17px 0;border-bottom:1px solid #223342;cursor:pointer}.timeline-item:last-child{border-bottom:0}.timeline-item.selected{background:#14252a;margin:0 -12px;padding-left:12px;padding-right:12px;border-radius:8px}.dot{width:10px;height:10px;border-radius:50%;background:#74e0b4;box-shadow:0 0 14px #74e0b4}.timeline-item>div:nth-child(2){flex:1}.timeline-item p{margin:4px 0 0}.timeline-item button{background:#162532;color:#dce8f1;border:1px solid #355164;border-radius:6px;padding:7px 10px}.player video{width:100%;background:#05090d;border-radius:8px;margin-top:16px}.player p{margin-bottom:0}.snapshot-strip{display:flex;gap:7px;overflow:auto;margin-top:16px}.snapshot-strip img{width:64px;height:48px;object-fit:cover;border-radius:5px;border:1px solid #294050}.card+.card{margin-top:20px}@media(max-width:800px){.layout{grid-template-columns:1fr}}
</style>
