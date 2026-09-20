<script lang="ts">
  import { onMount } from 'svelte';
  type Track = { id:string; object_class:string; first_seen:string; last_seen:string; duration_ms:number; movement_path: unknown[] };
  let tracks: Track[] = [];
  onMount(async () => { const response = await fetch('/api/tracks?limit=100'); if (response.ok) tracks = await response.json(); });
</script>
<p class="eyebrow">Identity continuity</p><h1>Tracking</h1><p class="muted">Persistent identities across adjacent frames.</p>
{#if tracks.length === 0}<div class="empty">No active tracks available.</div>{:else}<div class="grid" style="grid-template-columns:repeat(auto-fit,minmax(260px,1fr));margin-top:32px">{#each tracks as track}<article class="card"><div class="row"><strong>{track.object_class}</strong><span class="pill">{track.id.slice(0,8)}</span></div><div class="metric">{Math.round(track.duration_ms/1000)}s</div><p class="muted">{track.movement_path.length} path points</p><small class="muted">Last seen {new Date(track.last_seen).toLocaleString()}</small></article>{/each}</div>{/if}
