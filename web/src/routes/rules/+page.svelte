<script lang="ts">
  import { onMount } from 'svelte';
  import { api, apiError } from '$lib/api';
  type Rule = { id:string; name:string; description:string; enabled:boolean; severity:string; cooldown_seconds:number; conditions:{object_class?:string;zone_id?:string;observation_type?:string;behaviour_type?:string;minimum_duration_ms?:number}[] };
  let rules: Rule[] = []; let loading = true; let message = '';

  // New rule form
  let name = ''; let description = ''; let severity = 'warning'; let cooldown = 30;
  let objectClass = ''; let observationType = ''; let minDurationSeconds = 0; let saving = false;

  async function load() { const response = await api('/api/rules'); if (response.ok) rules = await response.json(); }
  onMount(async () => { try { await load(); } finally { loading = false; } });

  async function toggle(rule: Rule) {
    const response = await api(`/api/rules/${rule.id}`, { method:'PUT', json:{ enabled:!rule.enabled } });
    if (response.ok) { rule.enabled = !rule.enabled; rules = rules; } else message = await apiError(response);
  }

  async function create() {
    if (!name.trim()) { message = 'Rule name is required'; return; }
    saving = true;
    const condition: Record<string, unknown> = {};
    if (objectClass.trim()) condition.object_class = objectClass.trim();
    if (observationType.trim()) condition.observation_type = observationType.trim();
    if (minDurationSeconds > 0) condition.minimum_duration_ms = Math.round(minDurationSeconds * 1000);
    const response = await api('/api/rules', { method:'POST', json:{ name: name.trim(), description: description.trim(), severity, cooldown_seconds: cooldown, suppression_seconds: 0, enabled: true, conditions: [condition] } });
    message = response.ok ? `Created ${name.trim()}` : await apiError(response);
    if (response.ok) { name = ''; description = ''; objectClass = ''; observationType = ''; minDurationSeconds = 0; await load(); }
    saving = false;
  }

  async function remove(rule: Rule) {
    if (!confirm(`Delete rule "${rule.name}"?`)) return;
    const response = await api(`/api/rules/${rule.id}`, { method:'DELETE' });
    message = response.ok ? `${rule.name} deleted` : await apiError(response);
    if (response.ok) await load();
  }
</script>
<svelte:head><title>Rules · Objexel</title></svelte:head>
<p class="eyebrow">Decision layer</p><h1>Rules</h1><p class="muted">Observations describe what happened. Rules decide whether it matters.</p>
{#if message}<p class="pill">{message}</p>{/if}

<section class="card form">
  <div class="form-row">
    <input bind:value={name} placeholder="Rule name" />
    <input bind:value={description} placeholder="Description (optional)" />
    <select bind:value={severity}><option value="info">Info</option><option value="warning">Warning</option><option value="critical">Critical</option></select>
  </div>
  <div class="form-row">
    <input bind:value={objectClass} placeholder="Object class (e.g. person)" />
    <input bind:value={observationType} placeholder="Observation type (optional)" />
    <label class="num">Min duration (s)<input type="number" min="0" bind:value={minDurationSeconds} /></label>
    <label class="num">Cooldown (s)<input type="number" min="0" bind:value={cooldown} /></label>
    <button on:click={create} disabled={saving || !name.trim()}>{saving ? 'Creating…' : 'Add rule'}</button>
  </div>
</section>

{#if loading}<div class="empty">Loading rules…</div>{:else if rules.length === 0}<div class="empty">No rules configured. Add a rule for object, zone, confidence, or duration conditions.</div>{:else}<div class="grid" style="grid-template-columns:repeat(auto-fit,minmax(280px,1fr));margin-top:32px">{#each rules as rule}<article class="card"><div class="row"><div><h2>{rule.name}</h2><p class="muted">{rule.description || 'No description'}</p></div><button class:active={rule.enabled} class="toggle" on:click={() => toggle(rule)}>{rule.enabled ? 'Enabled' : 'Disabled'}</button></div><div class="rule-meta"><span class="pill">{rule.severity}</span><span>{rule.conditions.length} conditions</span><span>{rule.cooldown_seconds}s cooldown</span></div>{#each rule.conditions as condition}<div class="condition">{condition.object_class || 'any object'} · {condition.behaviour_type || condition.observation_type || 'any observation'}{#if condition.minimum_duration_ms} · {condition.minimum_duration_ms/1000}s minimum{/if}</div>{/each}<button class="danger" on:click={() => remove(rule)}>Delete rule</button></article>{/each}</div>{/if}
<style>h2{margin:0 0 5px}.form{margin-top:28px;display:grid;gap:12px}.form-row{display:flex;gap:8px;flex-wrap:wrap;align-items:center}.form-row input:not([type=number]){flex:1;min-width:160px}.form-row input,.form-row select{background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:10px}.num{display:flex;align-items:center;gap:6px;color:#8293a4;font-size:.8rem}.num input{width:90px}.form-row button{border:0;border-radius:6px;padding:10px 14px;background:#74e0b4;color:#0b1017;font-weight:700}.toggle{border:1px solid #355164;border-radius:99px;background:transparent;color:#8293a4;padding:6px 10px}.toggle.active{background:#18352f;color:#74e0b4;border-color:#28624f}.rule-meta{display:flex;gap:10px;align-items:center;margin:20px 0;color:#8293a4;font-size:.8rem}.condition{padding:9px 0;border-top:1px solid #223342;color:#c8d4df;font-size:.85rem}.danger{margin-top:16px;border:1px solid #6d2a35;background:transparent;color:#f0788a;border-radius:6px;padding:7px 12px;cursor:pointer}</style>
