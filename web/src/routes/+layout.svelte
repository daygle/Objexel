<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  type UpdateInfo = { current_version:string; latest_version?:string; update_available:boolean; release_url?:string; notes?:string };
  let update: UpdateInfo | null = null; let checking = false;
  async function loadUpdate() { const response = await fetch('/api/updates'); if (response.ok) update = await response.json(); }
  async function checkUpdate() { checking = true; const response = await fetch('/api/updates/check', { method:'POST' }); if (response.ok) update = await response.json(); checking = false; }
  onMount(loadUpdate);
</script>

<nav><a class="brand" href="/observations">OBJEXEL <span>EDGE INTELLIGENCE</span></a><div><a href="/observations">Observations</a><a href="/tracks">Tracking</a><a href="/detections">Detections</a><a href="/cameras">Cameras</a><a href="/zones">Zones</a><a href="/models">Models</a><a href="/pipelines">AI Pipelines</a><a href="/rules">Rules</a><a href="/events">Events</a><a href="/recordings">Recordings</a><a href="/search">Search</a><a href="/analytics">Analytics</a><a href="/behaviours">Behaviours</a><a href="/identities">Identities</a><a href="/intelligence">Intelligence</a><a href="/notifications">Notifications</a><a href="/users">Users</a><a href="/login">Sign in</a></div></nav>
{#if update?.update_available}<aside class="update"><strong>Objexel {update.latest_version} is available.</strong><span>Updates are operator-controlled: back up PostgreSQL and media before upgrading.</span>{#if update.release_url}<a href={update.release_url} target="_blank" rel="noreferrer">Read release notes</a>{/if}</aside>{/if}
<main><div class="update-check"><span class="muted">Running {update?.current_version ?? 'current release'}</span><button on:click={checkUpdate} disabled={checking}>{checking ? 'Checking…' : 'Check for updates'}</button></div><slot /></main>
<style>.update{display:flex;gap:14px;align-items:center;flex-wrap:wrap;padding:12px 6vw;background:#3b2d1d;color:#f0c56d;border-bottom:1px solid #72552b;font-size:.85rem}.update span{color:#f6dfac}.update a{color:#fff1c9}.update-check{display:flex;justify-content:flex-end;align-items:center;gap:10px;margin-bottom:20px;font-size:.75rem}.update-check button{border:1px solid #355164;border-radius:6px;background:#162532;color:#dce8f1;padding:6px 9px;font-size:.75rem}@media(max-width:900px){nav{height:auto;padding:18px 6vw;align-items:flex-start;gap:12px;flex-direction:column}nav div{display:flex;flex-wrap:wrap;gap:8px}nav a{margin-left:0}.update-check{justify-content:flex-start}}
</style>
