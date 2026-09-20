<script lang="ts">
  import { onMount } from 'svelte';
  type Action = { id:string; name:string; action_type:string; enabled:boolean; provider_id?:string };
  type Provider = { id:string; provider_type:string; enabled:boolean };
  let actions: Action[] = []; let providers: Provider[] = []; let name = ''; let providerType = 'internal'; let configuration = '{}'; let message = '';
  async function load() { const [a, p] = await Promise.all([fetch('/api/actions'), fetch('/api/notification-providers')]); if (a.ok) actions = await a.json(); if (p.ok) providers = await p.json(); }
  async function createProvider() { let parsed: unknown; try { parsed = JSON.parse(configuration); } catch { message = 'Configuration must be valid JSON'; return; } const response = await fetch('/api/notification-providers', { method:'POST', headers:{'content-type':'application/json'}, body:JSON.stringify({ provider_type:providerType, enabled:true, configuration:parsed }) }); message = response.ok ? 'Provider created' : await response.text(); await load(); }
  async function createAction() { const provider = providers.find((item) => item.provider_type === providerType); const response = await fetch('/api/actions', { method:'POST', headers:{'content-type':'application/json'}, body:JSON.stringify({ name, action_type:providerType, provider_id:provider?.id ?? null, enabled:true, configuration:{} }) }); message = response.ok ? 'Action created' : await response.text(); if (response.ok) name = ''; await load(); }
  onMount(load);
</script>
<svelte:head><title>Actions · Objexel</title></svelte:head>
<p class="eyebrow">Response automation</p><h1>Actions</h1><p class="muted">Connect rule events to delivery providers. Provider credentials remain in PostgreSQL configuration and are never logged.</p>
{#if message}<p class="pill">{message}</p>{/if}
<section class="card form"><input bind:value={name} placeholder="Action name" /><select bind:value={providerType}><option value="internal">Internal</option><option value="email">Email</option><option value="webhook">Webhook</option><option value="mqtt">MQTT</option></select><textarea bind:value={configuration} placeholder='Provider JSON, e.g. {"url":"https://…"}'></textarea><button on:click={createProvider}>Create provider</button><button on:click={createAction} disabled={!name}>Create action</button></section>
<div class="grid" style="grid-template-columns:repeat(auto-fit,minmax(280px,1fr));margin-top:24px">{#each actions as action}<article class="card"><div class="row"><strong>{action.name}</strong><span class="pill">{action.enabled ? 'Enabled' : 'Disabled'}</span></div><p class="muted">{action.action_type} · {action.provider_id ? action.provider_id.slice(0,8) : 'no provider'}</p></article>{/each}</div>
<style>.form{display:flex;gap:8px;flex-wrap:wrap;margin-top:28px}.form input,.form select,.form textarea{background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:10px}.form input{flex:1;min-width:180px}.form textarea{min-width:260px;min-height:44px}.form button{border:0;border-radius:6px;padding:10px 12px;background:#74e0b4;color:#0b1017;font-weight:700}</style>
