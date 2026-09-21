<script lang="ts">
  import { onMount } from 'svelte';
  import { api, apiError } from '$lib/api';
  type UpdateInfo = { current_version:string; latest_version?:string; update_available:boolean; release_url?:string; notes?:string };
  type Metrics = { uptime_seconds:number; http_requests_total:number; cameras_total?:number; cameras_online?:number; active_tracks?:number; recent_detections?:number; database?:string; pipeline_cameras?:unknown[] };

  let update: UpdateInfo | null = null;
  let metrics: Metrics | null = null;
  let checking = false;
  let message = '';

  async function load() {
    const [updateRes, metricsRes] = await Promise.all([
      api('/api/updates', { redirectOnUnauthorized:false }),
      api('/metrics', { redirectOnUnauthorized:false })
    ]);
    if (updateRes.ok) update = await updateRes.json();
    if (metricsRes.ok) metrics = await metricsRes.json();
  }

  async function checkUpdate() {
    checking = true; message = '';
    const response = await api('/api/updates/check', { method:'POST', redirectOnUnauthorized:false });
    if (response.ok) { update = await response.json(); message = update?.update_available ? `Update available: ${update.latest_version}` : 'Already running the latest version'; }
    else message = await apiError(response);
    checking = false;
  }

  function formatUptime(seconds: number): string {
    const d = Math.floor(seconds / 86400); const h = Math.floor((seconds % 86400) / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (d > 0) return `${d}d ${h}h ${m}m`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  onMount(load);
</script>

<svelte:head><title>Settings · Objexel</title></svelte:head>
<p class="eyebrow">System</p>
<h1>Settings</h1>
<p class="muted">System information, updates, and configuration.</p>

{#if message}<p class="pill">{message}</p>{/if}

<div class="grid" style="grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); margin-top: 32px;">

  <section class="card">
    <h2>System</h2>
    <div class="info-rows">
      <div class="info-row"><span class="label">Version</span><span>{update?.current_version ?? '-'}</span></div>
      <div class="info-row"><span class="label">Uptime</span><span>{metrics ? formatUptime(metrics.uptime_seconds) : '-'}</span></div>
      <div class="info-row"><span class="label">HTTP requests</span><span>{metrics?.http_requests_total?.toLocaleString() ?? '-'}</span></div>
      <div class="info-row"><span class="label">Database</span><span class:ok={metrics?.database === 'ok'}>{metrics?.database ?? '-'}</span></div>
    </div>
  </section>

  <section class="card">
    <h2>Cameras &amp; Detection</h2>
    <div class="info-rows">
      <div class="info-row"><span class="label">Cameras</span><span>{metrics?.cameras_total ?? 0} total · {metrics?.cameras_online ?? 0} online</span></div>
      <div class="info-row"><span class="label">Active tracks</span><span>{metrics?.active_tracks ?? 0}</span></div>
      <div class="info-row"><span class="label">Recent detections</span><span>{metrics?.recent_detections ?? 0}</span></div>
      <div class="info-row"><span class="label">Pipeline cameras</span><span>{metrics?.pipeline_cameras?.length ?? 0}</span></div>
    </div>
  </section>

  <section class="card">
    <h2>Updates</h2>
    <div class="info-rows">
      <div class="info-row"><span class="label">Current</span><span>{update?.current_version ?? '-'}</span></div>
      <div class="info-row">
        <span class="label">Latest</span>
        <span>{#if update?.latest_version}{update.latest_version}{#if update.update_available} <span class="badge">available</span>{/if}{:else}-{/if}</span>
      </div>
      {#if update?.release_url}
        <div class="info-row"><span class="label">Release</span><a href={update.release_url} target="_blank" rel="noreferrer">View notes</a></div>
      {/if}
    </div>
    <div class="actions" style="margin-top: 16px;">
      <button on:click={checkUpdate} disabled={checking}>{checking ? 'Checking…' : 'Check for updates'}</button>
    </div>
    {#if update?.update_available}
      <p class="muted small" style="margin-top: 12px;">Back up PostgreSQL, configuration, and media before upgrading. Then pull the new image and redeploy.</p>
    {/if}
  </section>

</div>

<style>
  h2 { margin: 0 0 16px; }
  .info-rows { display: grid; gap: 0; }
  .info-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 10px 0; border-bottom: 1px solid #1d2a38;
  }
  .info-row:last-child { border-bottom: none; }
  .label { color: #8293a4; font-size: .85rem; }
  .ok { color: #74e0b4; }
  .badge {
    display: inline-block; padding: 2px 8px; border-radius: 99px;
    background: #3b2d1d; color: #f0c56d; font-size: .7rem; font-weight: 600;
    margin-left: 6px;
  }
  .actions button {
    background: #74e0b4; color: #0b1017; border: 0; border-radius: 6px;
    padding: 9px 14px; font-weight: 700; font-size: .85rem; cursor: pointer;
  }
  .actions button:disabled { opacity: .5; cursor: not-allowed; }
  .small { font-size: .8rem; }
</style>
