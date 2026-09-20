<script lang="ts">
  import { onMount } from 'svelte';

  type Notification = { id:string; title:string; body:string; read:boolean; created_at:string };
  type Execution = { id:string; action_id:string; event_id:string; status:string; execution_time_ms?:number; error_message?:string; created_at:string };
  type Provider = { id:string; provider_type:string; enabled:boolean };

  let notifications: Notification[] = [];
  let executions: Execution[] = [];
  let providers: Provider[] = [];

  async function load() {
    const responses = await Promise.all([fetch('/api/notifications?limit=50'), fetch('/api/action-executions?limit=50'), fetch('/api/notification-providers')]);
    if (responses[0].ok) notifications = await responses[0].json();
    if (responses[1].ok) executions = await responses[1].json();
    if (responses[2].ok) providers = await responses[2].json();
  }

  onMount(load);
</script>

<svelte:head><title>Notifications · Objexel</title></svelte:head>
<p class="eyebrow">Actions & delivery</p>
<h1>Notifications</h1>
<p class="muted">Events can fan out to internal notifications, SMTP email, signed webhooks, and MQTT.</p>

<div class="grid metrics">
  <div class="card"><span class="muted">Internal notifications</span><div class="metric">{notifications.length}</div></div>
  <div class="card"><span class="muted">Executions</span><div class="metric">{executions.length}</div></div>
  <div class="card"><span class="muted">Providers</span><div class="metric">{providers.filter((provider) => provider.enabled).length}</div></div>
</div>

<section class="card section">
  <div class="row"><h2>Delivery history</h2><a href="/actions">Manage actions →</a></div>
  {#if executions.length === 0}<div class="empty">No action executions yet.</div>{:else}<table class="table"><thead><tr><th>Status</th><th>Action</th><th>Event</th><th>Latency</th><th>Time</th></tr></thead><tbody>{#each executions as execution}<tr><td><span class:failed={execution.status === 'failed'} class="status">{execution.status}</span></td><td class="muted">{execution.action_id.slice(0,8)}</td><td class="muted">{execution.event_id.slice(0,8)}</td><td class="muted">{execution.execution_time_ms ?? 0} ms</td><td class="muted">{new Date(execution.created_at).toLocaleString()}</td></tr>{/each}</tbody></table>{/if}
</section>

<section class="card section">
  <h2>Internal notifications</h2>
  {#if notifications.length === 0}<div class="empty">No internal notifications yet.</div>{:else}{#each notifications as notification}<article class="notification"><div class="row"><strong>{notification.title}</strong><small>{new Date(notification.created_at).toLocaleString()}</small></div><p class="muted">{notification.body}</p></article>{/each}{/if}
</section>

<style>
  h2 { margin: 0; }
  .metrics { grid-template-columns:repeat(3,1fr); margin:32px 0; }
  .section { margin-top:20px; }
  .section a { color:#74e0b4; text-decoration:none; }
  .status { padding:5px 9px; border-radius:99px; background:#18352f; color:#74e0b4; font-size:.75rem; }
  .status.failed { background:#401f29; color:#f0788a; }
  .notification { padding:14px 0; border-bottom:1px solid #223342; }
  small { color:#71869b; }
  @media(max-width:700px){ .metrics { grid-template-columns:1fr; } }
</style>
