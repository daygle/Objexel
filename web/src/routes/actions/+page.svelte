<script lang="ts">
  import { onMount } from 'svelte';
  import { api, apiError } from '$lib/api';
  type Action = { id:string; name:string; action_type:string; enabled:boolean; provider_id?:string|null; template_id?:string|null; configuration:unknown };
  type Provider = { id:string; provider_type:string; enabled:boolean; configuration:unknown };
  type Template = { id:string; name:string; subject:string; body:string; html:boolean };

  let actions: Action[] = []; let providers: Provider[] = []; let templates: Template[] = []; let message = '';

  // Create-provider form
  let providerType = 'internal'; let providerConfig = '{}';
  // Create-template form
  let tplName = ''; let tplSubject = ''; let tplBody = ''; let tplHtml = false;
  // Create-action form
  let name = ''; let actionType = 'internal'; let actionProvider = ''; let actionTemplate = '';
  // Test form
  let testProviderId = ''; let testTemplateId = '';

  // Inline editors
  let providerEdit: string | null = null; let providerEditConfig = '';
  let actionEdit: string | null = null; let aeName = ''; let aeProvider = ''; let aeTemplate = ''; let aeEnabled = true; let aeConfig = '{}';

  async function load() {
    const [a, p, t] = await Promise.all([api('/api/actions'), api('/api/notification-providers'), api('/api/notification-templates')]);
    if (a.ok) actions = await a.json(); if (p.ok) providers = await p.json(); if (t.ok) templates = await t.json();
  }
  onMount(load);

  function parseJson(text: string): unknown | undefined { try { return JSON.parse(text); } catch { message = 'Configuration must be valid JSON'; return undefined; } }
  const providerName = (id?: string|null) => id ? (providers.find((item) => item.id === id)?.provider_type ?? id.slice(0,8)) : 'no provider';
  const templateName = (id?: string|null) => id ? (templates.find((item) => item.id === id)?.name ?? id.slice(0,8)) : 'no template';

  async function createProvider() {
    const parsed = parseJson(providerConfig); if (parsed === undefined) return;
    const response = await api('/api/notification-providers', { method:'POST', json:{ provider_type:providerType, enabled:true, configuration:parsed } });
    message = response.ok ? 'Provider created' : await apiError(response); if (response.ok) { providerConfig = '{}'; await load(); }
  }
  function startProviderEdit(provider: Provider) { providerEdit = provider.id; providerEditConfig = JSON.stringify(provider.configuration ?? {}, null, 2); }
  async function saveProviderConfig(provider: Provider) {
    const parsed = parseJson(providerEditConfig); if (parsed === undefined) return;
    const response = await api(`/api/notification-providers/${provider.id}`, { method:'PUT', json:{ configuration:parsed } });
    message = response.ok ? 'Provider configuration saved' : await apiError(response); if (response.ok) { providerEdit = null; await load(); }
  }
  async function toggleProvider(provider: Provider) { const response = await api(`/api/notification-providers/${provider.id}`, { method:'PUT', json:{ enabled:!provider.enabled } }); message = response.ok ? `Provider ${provider.enabled ? 'disabled' : 'enabled'}` : await apiError(response); await load(); }
  async function validateProvider(provider: Provider) { message = `Validating ${provider.provider_type} configuration…`; const response = await api(`/api/notification-providers/${provider.id}/validate`, { method:'POST' }); message = response.ok ? 'Provider configuration is valid' : await apiError(response); }

  async function createTemplate() {
    if (!tplName.trim() || !tplBody.trim()) { message = 'Template name and body are required'; return; }
    const response = await api('/api/notification-templates', { method:'POST', json:{ name:tplName.trim(), subject:tplSubject, body:tplBody, html:tplHtml } });
    message = response.ok ? 'Template created' : await apiError(response); if (response.ok) { tplName = ''; tplSubject = ''; tplBody = ''; tplHtml = false; await load(); }
  }

  async function createAction() {
    if (!name.trim()) { message = 'Action name is required'; return; }
    const provider = actionProvider || providers.find((item) => item.provider_type === actionType)?.id || null;
    const response = await api('/api/actions', { method:'POST', json:{ name:name.trim(), action_type:actionType, provider_id:provider, template_id:actionTemplate || null, enabled:true, configuration:{} } });
    message = response.ok ? 'Action created' : await apiError(response); if (response.ok) { name = ''; actionProvider = ''; actionTemplate = ''; await load(); }
  }
  function startActionEdit(action: Action) { actionEdit = action.id; aeName = action.name; aeProvider = action.provider_id ?? ''; aeTemplate = action.template_id ?? ''; aeEnabled = action.enabled; aeConfig = JSON.stringify(action.configuration ?? {}, null, 2); }
  async function saveAction(action: Action) {
    const parsed = parseJson(aeConfig); if (parsed === undefined) return;
    const response = await api(`/api/actions/${action.id}`, { method:'PUT', json:{ name:aeName.trim(), provider_id:aeProvider || null, template_id:aeTemplate || null, enabled:aeEnabled, configuration:parsed } });
    message = response.ok ? `${aeName.trim()} updated` : await apiError(response); if (response.ok) { actionEdit = null; await load(); }
  }
  async function toggleAction(action: Action) { const response = await api(`/api/actions/${action.id}`, { method:'PUT', json:{ enabled:!action.enabled } }); message = response.ok ? `${action.name} ${action.enabled ? 'disabled' : 'enabled'}` : await apiError(response); await load(); }
  async function removeAction(action: Action) { if (!confirm(`Delete action "${action.name}"?`)) return; const response = await api(`/api/actions/${action.id}`, { method:'DELETE' }); message = response.ok ? `${action.name} deleted` : await apiError(response); if (response.ok) { if (actionEdit === action.id) actionEdit = null; await load(); } }

  async function sendTest() {
    if (!testProviderId) { message = 'Choose a provider to test'; return; }
    message = 'Sending test notification…';
    const response = await api('/api/test-notification', { method:'POST', json:{ provider_id:testProviderId, template_id:testTemplateId || null } });
    message = response.ok ? 'Test notification sent' : await apiError(response);
  }
</script>
<svelte:head><title>Actions · Objexel</title></svelte:head>
<p class="eyebrow">Response automation</p><h1>Actions</h1><p class="muted">Connect rule events to delivery providers and templates. Provider credentials remain in PostgreSQL configuration and are never logged.</p>
{#if message}<p class="pill">{message}</p>{/if}

<section class="card section"><h2>Providers</h2>
  <div class="form"><select bind:value={providerType}><option value="internal">Internal</option><option value="email">Email</option><option value="webhook">Webhook</option><option value="mqtt">MQTT</option></select><textarea bind:value={providerConfig} placeholder="Provider JSON config, e.g. an https webhook URL or MQTT topic"></textarea><button on:click={createProvider}>Create provider</button></div>
  {#each providers as provider}
    <div class="line"><div><strong>{provider.provider_type}</strong><small class="muted">{provider.id.slice(0,8)}</small></div><span class="pill" class:off={!provider.enabled}>{provider.enabled ? 'Enabled' : 'Disabled'}</span><div class="line-actions"><button class="ghost" on:click={() => validateProvider(provider)}>Validate</button><button class="ghost" on:click={() => startProviderEdit(provider)}>Edit config</button><button class="ghost" on:click={() => toggleProvider(provider)}>{provider.enabled ? 'Disable' : 'Enable'}</button></div></div>
    {#if providerEdit === provider.id}<div class="edit-block"><textarea bind:value={providerEditConfig}></textarea><div class="line-actions"><button on:click={() => saveProviderConfig(provider)}>Save config</button><button class="ghost" on:click={() => providerEdit = null}>Cancel</button></div></div>{/if}
  {/each}
</section>

<section class="card section"><h2>Templates</h2>
  <div class="form"><input bind:value={tplName} placeholder="Template name" /><input bind:value={tplSubject} placeholder="Subject (email)" /><textarea bind:value={tplBody} placeholder="Body text - event summary tokens are substituted at send time"></textarea><label class="check"><input type="checkbox" bind:checked={tplHtml} /> HTML body</label><button on:click={createTemplate} disabled={!tplName.trim() || !tplBody.trim()}>Create template</button></div>
  {#if templates.length === 0}<p class="muted small">No templates yet. Actions without a template send a default event summary.</p>{:else}{#each templates as template}<div class="line"><div><strong>{template.name}</strong><small class="muted">{template.subject || 'no subject'} · {template.html ? 'HTML' : 'text'}</small></div></div>{/each}{/if}
</section>

<section class="card section"><h2>Send test notification</h2>
  <div class="form"><select bind:value={testProviderId}><option value="">Choose provider…</option>{#each providers as provider}<option value={provider.id}>{provider.provider_type} · {provider.id.slice(0,8)}</option>{/each}</select><select bind:value={testTemplateId}><option value="">No template</option>{#each templates as template}<option value={template.id}>{template.name}</option>{/each}</select><button on:click={sendTest} disabled={!testProviderId}>Send test</button></div>
</section>

<section class="card section"><h2>Actions</h2>
  <div class="form"><input bind:value={name} placeholder="Action name" /><select bind:value={actionType}><option value="internal">Internal</option><option value="email">Email</option><option value="webhook">Webhook</option><option value="mqtt">MQTT</option></select><select bind:value={actionProvider}><option value="">Provider by type</option>{#each providers as provider}<option value={provider.id}>{provider.provider_type} · {provider.id.slice(0,8)}</option>{/each}</select><select bind:value={actionTemplate}><option value="">No template</option>{#each templates as template}<option value={template.id}>{template.name}</option>{/each}</select><button on:click={createAction} disabled={!name.trim()}>Create action</button></div>
</section>

<div class="grid" style="grid-template-columns:repeat(auto-fit,minmax(300px,1fr));margin-top:24px">{#each actions as action}<article class="card">
  {#if actionEdit === action.id}
    <input class="full" bind:value={aeName} placeholder="Action name" />
    <select class="full" bind:value={aeProvider}><option value="">No provider</option>{#each providers as provider}<option value={provider.id}>{provider.provider_type} · {provider.id.slice(0,8)}</option>{/each}</select>
    <select class="full" bind:value={aeTemplate}><option value="">No template</option>{#each templates as template}<option value={template.id}>{template.name}</option>{/each}</select>
    <label class="check"><input type="checkbox" bind:checked={aeEnabled} /> Enabled</label>
    <textarea class="full" bind:value={aeConfig}></textarea>
    <div class="line-actions"><button on:click={() => saveAction(action)}>Save</button><button class="ghost" on:click={() => actionEdit = null}>Cancel</button></div>
  {:else}
    <div class="row"><strong>{action.name}</strong><span class="pill" class:off={!action.enabled}>{action.enabled ? 'Enabled' : 'Disabled'}</span></div>
    <p class="muted">{action.action_type} · {providerName(action.provider_id)} · {templateName(action.template_id)}</p>
    <div class="line-actions"><button class="ghost" on:click={() => startActionEdit(action)}>Edit</button><button class="ghost" on:click={() => toggleAction(action)}>{action.enabled ? 'Disable' : 'Enable'}</button><button class="danger" on:click={() => removeAction(action)}>Delete</button></div>
  {/if}
</article>{/each}</div>
<style>
  h2{margin:0 0 12px}
  .section{margin-top:24px}
  .form{display:flex;gap:8px;flex-wrap:wrap;align-items:center;margin-bottom:8px}
  .form input,.form select,.form textarea{background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:10px}
  .form input{flex:1;min-width:160px}.form textarea{min-width:260px;min-height:44px;flex:1}
  .check{display:flex;align-items:center;gap:6px;color:#8293a4;font-size:.85rem}
  .form button{border:0;border-radius:6px;padding:10px 12px;background:#74e0b4;color:#0b1017;font-weight:700}
  .line{display:flex;align-items:center;gap:14px;padding:12px 0;border-bottom:1px solid #223342}.line>div:first-child{flex:1}.line small{display:block;margin-top:3px}
  .pill.off{background:#3b2d1d;color:#f0c56d}
  .line-actions{display:flex;gap:6px;flex-wrap:wrap}
  .line-actions button{border:1px solid #355164;border-radius:6px;background:#162532;color:#dce8f1;padding:7px 10px;font-size:.8rem}
  .line-actions button.ghost{background:transparent}.line-actions button.danger{border-color:#6d2a35;color:#f0788a}
  .edit-block{padding:10px 0}.edit-block textarea{width:100%;min-height:90px;background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:10px;font-family:ui-monospace,monospace}
  .full{width:100%;margin-bottom:8px;background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:9px}
  textarea.full{min-height:80px;font-family:ui-monospace,monospace}
  .small{font-size:.8rem}
</style>
