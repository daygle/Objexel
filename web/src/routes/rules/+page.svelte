<script lang="ts">
  import { onMount } from 'svelte';
  import { api, apiError } from '$lib/api';

  type Condition = { object_class?:string; zone_id?:string; observation_type?:string; behaviour_type?:string; confidence_threshold?:number; minimum_duration_ms?:number };
  type Rule = { id:string; name:string; description:string; enabled:boolean; severity:string; cooldown_seconds:number; suppression_seconds:number; conditions:Condition[]; action_ids:string[] };
  type Action = { id:string; name:string; action_type:string };

  let rules: Rule[] = []; let actions: Action[] = []; let loading = true; let message = '';

  // Editor state — id null means "new rule".
  type ConditionDraft = { object_class:string; observation_type:string; behaviour_type:string; confidence:string; duration:string };
  type Draft = { id:string|null; name:string; description:string; severity:string; cooldown:number; suppression:number; conditions:ConditionDraft[]; actionIds:string[] };
  const emptyCondition = (): ConditionDraft => ({ object_class:'', observation_type:'', behaviour_type:'', confidence:'', duration:'' });
  const newDraft = (): Draft => ({ id:null, name:'', description:'', severity:'warning', cooldown:30, suppression:0, conditions:[emptyCondition()], actionIds:[] });
  let draft: Draft = newDraft();
  let saving = false;

  async function load() {
    const [ruleResponse, actionResponse] = await Promise.all([api('/api/rules'), api('/api/actions')]);
    if (ruleResponse.ok) rules = await ruleResponse.json();
    if (actionResponse.ok) actions = await actionResponse.json();
  }
  onMount(async () => { try { await load(); } finally { loading = false; } });

  function startNew() { draft = newDraft(); }
  function editRule(rule: Rule) {
    draft = {
      id: rule.id, name: rule.name, description: rule.description ?? '', severity: rule.severity,
      cooldown: rule.cooldown_seconds, suppression: rule.suppression_seconds,
      conditions: (rule.conditions.length ? rule.conditions : [{}]).map((c) => ({
        object_class: c.object_class ?? '', observation_type: c.observation_type ?? '', behaviour_type: c.behaviour_type ?? '',
        confidence: c.confidence_threshold != null ? String(Math.round(c.confidence_threshold * 100)) : '',
        duration: c.minimum_duration_ms != null ? String(c.minimum_duration_ms / 1000) : ''
      })),
      actionIds: [...rule.action_ids]
    };
    if (typeof window !== 'undefined') window.scrollTo({ top: 0, behavior: 'smooth' });
  }
  function addCondition() { draft.conditions = [...draft.conditions, emptyCondition()]; }
  function removeCondition(index: number) { draft.conditions = draft.conditions.filter((_, i) => i !== index); }
  function toggleActionLink(id: string) { draft.actionIds = draft.actionIds.includes(id) ? draft.actionIds.filter((item) => item !== id) : [...draft.actionIds, id]; }

  function buildConditions(): Condition[] {
    return draft.conditions.map((row) => {
      const condition: Condition = {};
      if (row.object_class.trim()) condition.object_class = row.object_class.trim();
      if (row.observation_type.trim()) condition.observation_type = row.observation_type.trim();
      if (row.behaviour_type.trim()) condition.behaviour_type = row.behaviour_type.trim();
      const confidence = Number(row.confidence);
      if (row.confidence.trim() && !Number.isNaN(confidence)) condition.confidence_threshold = Math.min(1, Math.max(0, confidence / 100));
      const duration = Number(row.duration);
      if (row.duration.trim() && !Number.isNaN(duration) && duration > 0) condition.minimum_duration_ms = Math.round(duration * 1000);
      return condition;
    });
  }

  async function save() {
    if (!draft.name.trim()) { message = 'Rule name is required'; return; }
    saving = true;
    const payload = {
      name: draft.name.trim(), description: draft.description.trim(), severity: draft.severity,
      cooldown_seconds: draft.cooldown, suppression_seconds: draft.suppression,
      conditions: buildConditions(), action_ids: draft.actionIds
    };
    const response = draft.id
      ? await api(`/api/rules/${draft.id}`, { method:'PUT', json: payload })
      : await api('/api/rules', { method:'POST', json: { enabled:true, ...payload } });
    message = response.ok ? (draft.id ? 'Rule updated' : `Created ${draft.name.trim()}`) : await apiError(response);
    if (response.ok) { draft = newDraft(); await load(); }
    saving = false;
  }

  async function toggle(rule: Rule) {
    const response = await api(`/api/rules/${rule.id}`, { method:'PUT', json:{ enabled:!rule.enabled } });
    if (response.ok) { rule.enabled = !rule.enabled; rules = rules; } else message = await apiError(response);
  }
  async function remove(rule: Rule) {
    if (!confirm(`Delete rule "${rule.name}"?`)) return;
    const response = await api(`/api/rules/${rule.id}`, { method:'DELETE' });
    message = response.ok ? `${rule.name} deleted` : await apiError(response);
    if (response.ok) { if (draft.id === rule.id) draft = newDraft(); await load(); }
  }
  const actionName = (id: string) => actions.find((item) => item.id === id)?.name ?? id.slice(0, 8);
</script>
<svelte:head><title>Rules · Objexel</title></svelte:head>
<p class="eyebrow">Decision layer</p><h1>Rules</h1><p class="muted">Observations describe what happened. Rules decide whether it matters — and which actions fire in response.</p>
{#if message}<p class="pill">{message}</p>{/if}

<section class="card editor">
  <div class="row"><h2>{draft.id ? 'Edit rule' : 'New rule'}</h2>{#if draft.id}<button class="ghost" on:click={startNew}>Cancel edit</button>{/if}</div>
  <div class="form-row">
    <input bind:value={draft.name} placeholder="Rule name" />
    <input bind:value={draft.description} placeholder="Description (optional)" />
    <select bind:value={draft.severity}><option value="info">Info</option><option value="warning">Warning</option><option value="critical">Critical</option></select>
    <label class="num">Cooldown (s)<input type="number" min="0" bind:value={draft.cooldown} /></label>
    <label class="num">Suppression (s)<input type="number" min="0" bind:value={draft.suppression} /></label>
  </div>

  <div class="subhead"><strong>Conditions</strong><button class="ghost" on:click={addCondition}>+ Add condition</button></div>
  {#each draft.conditions as condition, index}
    <div class="condition-row">
      <input bind:value={condition.object_class} placeholder="Object class" />
      <input bind:value={condition.observation_type} placeholder="Observation type" />
      <input bind:value={condition.behaviour_type} placeholder="Behaviour type" />
      <label class="num">Min conf %<input type="number" min="0" max="100" bind:value={condition.confidence} /></label>
      <label class="num">Min dur s<input type="number" min="0" bind:value={condition.duration} /></label>
      {#if draft.conditions.length > 1}<button class="del" on:click={() => removeCondition(index)} title="Remove condition">✕</button>{/if}
    </div>
  {/each}

  <div class="subhead"><strong>Trigger actions</strong><span class="muted">Selected actions run when this rule fires</span></div>
  {#if actions.length === 0}<p class="muted small">No actions defined yet. Create them on the <a href="/actions">Actions</a> page.</p>{:else}
    <div class="action-picker">{#each actions as action}<label class="pick"><input type="checkbox" checked={draft.actionIds.includes(action.id)} on:change={() => toggleActionLink(action.id)} /> {action.name} <span class="muted">· {action.action_type}</span></label>{/each}</div>
  {/if}

  <div class="editor-actions"><button on:click={save} disabled={saving || !draft.name.trim()}>{saving ? 'Saving…' : (draft.id ? 'Save changes' : 'Create rule')}</button></div>
</section>

{#if loading}<div class="empty">Loading rules…</div>{:else if rules.length === 0}<div class="empty">No rules configured. Add a rule above for object, zone, confidence, or duration conditions.</div>{:else}<div class="grid" style="grid-template-columns:repeat(auto-fit,minmax(300px,1fr));margin-top:32px">{#each rules as rule}<article class="card"><div class="row"><div><h2>{rule.name}</h2><p class="muted">{rule.description || 'No description'}</p></div><button class:active={rule.enabled} class="toggle" on:click={() => toggle(rule)}>{rule.enabled ? 'Enabled' : 'Disabled'}</button></div><div class="rule-meta"><span class="pill">{rule.severity}</span><span>{rule.conditions.length} conditions</span><span>{rule.cooldown_seconds}s cooldown</span></div>{#each rule.conditions as condition}<div class="condition">{condition.object_class || 'any object'} · {condition.behaviour_type || condition.observation_type || 'any observation'}{#if condition.confidence_threshold != null} · ≥{Math.round(condition.confidence_threshold*100)}%{/if}{#if condition.minimum_duration_ms} · {condition.minimum_duration_ms/1000}s minimum{/if}</div>{/each}<div class="links">{#if rule.action_ids.length === 0}<span class="muted small">No actions linked — this rule records events but triggers nothing.</span>{:else}{#each rule.action_ids as id}<span class="pill link">{actionName(id)}</span>{/each}{/if}</div><div class="editor-actions"><button class="ghost" on:click={() => editRule(rule)}>Edit</button><button class="danger" on:click={() => remove(rule)}>Delete</button></div></article>{/each}</div>{/if}
<style>
  h2{margin:0 0 5px}
  .editor{margin-top:28px;display:grid;gap:14px}
  .form-row{display:flex;gap:8px;flex-wrap:wrap;align-items:center}
  .form-row input:not([type=number]){flex:1;min-width:160px}
  .form-row input,.form-row select{background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:10px}
  .num{display:flex;align-items:center;gap:6px;color:#8293a4;font-size:.8rem;white-space:nowrap}.num input{width:90px}
  .subhead{display:flex;justify-content:space-between;align-items:center;gap:12px;border-top:1px solid #223342;padding-top:12px}
  .condition-row{display:flex;gap:8px;flex-wrap:wrap;align-items:center}
  .condition-row input:not([type=number]){flex:1;min-width:130px;background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:9px}
  .condition-row .num input{background:#0b1017;color:#e7edf5;border:1px solid #355164;border-radius:6px;padding:9px}
  .del{border:1px solid #6d2a35;background:transparent;color:#f0788a;border-radius:6px;padding:6px 9px;cursor:pointer}
  .action-picker{display:flex;flex-wrap:wrap;gap:12px}
  .pick{display:flex;align-items:center;gap:6px;color:#c8d4df;font-size:.85rem}
  .editor-actions{display:flex;gap:8px;flex-wrap:wrap;margin-top:6px}
  .editor-actions button{border:0;border-radius:6px;padding:9px 14px;background:#74e0b4;color:#0b1017;font-weight:700}
  .editor-actions button.ghost{background:transparent;border:1px solid #355164;color:#dce8f1}
  .editor-actions button.danger{background:transparent;border:1px solid #6d2a35;color:#f0788a}
  .toggle{border:1px solid #355164;border-radius:99px;background:transparent;color:#8293a4;padding:6px 10px}.toggle.active{background:#18352f;color:#74e0b4;border-color:#28624f}
  .rule-meta{display:flex;gap:10px;align-items:center;margin:20px 0;color:#8293a4;font-size:.8rem}
  .condition{padding:9px 0;border-top:1px solid #223342;color:#c8d4df;font-size:.85rem}
  .links{display:flex;gap:6px;flex-wrap:wrap;margin:14px 0 4px}.pill.link{background:#1b2f3a;color:#8fd6ff}
  .small{font-size:.8rem}.row .ghost{border:1px solid #355164;border-radius:6px;background:transparent;color:#dce8f1;padding:6px 10px}
</style>
