<script lang="ts">
  import { onMount } from 'svelte';
  import { api, apiError } from '$lib/api';
  type User = { id:string; username:string; email?:string; role:string; enabled:boolean; created_at:string };
  let users: User[] = []; let message = ''; let loaded = false;

  // New user form
  let username = ''; let email = ''; let password = ''; let role = 'viewer'; let saving = false;

  // Inline edit
  let editingId: string | null = null;
  let editEmail = ''; let editRole = 'viewer'; let editEnabled = true; let editPassword = '';

  async function load() {
    const response = await api('/api/users');
    if (response.ok) { users = await response.json(); loaded = true; message = ''; }
    else message = await apiError(response);
  }

  async function create() {
    if (!username.trim() || !password) { message = 'Username and password are required'; return; }
    saving = true;
    const response = await api('/api/users', { method:'POST', json:{ username: username.trim(), email: email.trim() || null, password, role } });
    message = response.ok ? `Created ${username.trim()}` : await apiError(response);
    if (response.ok) { username = ''; email = ''; password = ''; role = 'viewer'; await load(); }
    saving = false;
  }

  function startEdit(user: User) {
    editingId = user.id; editEmail = user.email ?? ''; editRole = user.role; editEnabled = user.enabled; editPassword = '';
  }
  function cancelEdit() { editingId = null; }

  async function saveEdit(user: User) {
    const payload: Record<string, unknown> = { email: editEmail.trim() || null, role: editRole, enabled: editEnabled };
    if (editPassword) payload.password = editPassword;
    const response = await api(`/api/users/${user.id}`, { method:'PUT', json: payload });
    message = response.ok ? `${user.username} updated` : await apiError(response);
    if (response.ok) { editingId = null; await load(); }
  }

  async function remove(user: User) {
    if (!confirm(`Delete user "${user.username}"?`)) return;
    const response = await api(`/api/users/${user.id}`, { method:'DELETE' });
    message = response.ok ? `${user.username} deleted` : await apiError(response);
    if (response.ok) await load();
  }

  onMount(load);
</script>
<svelte:head><title>Users · Objexel</title></svelte:head>
<p class="eyebrow">Access control</p><h1>Users</h1><p class="muted">Administrator-only local accounts and roles.</p>
{#if message}<p class="pill">{message}</p>{/if}

{#if loaded}
<section class="card form">
  <div class="form-row">
    <input bind:value={username} placeholder="Username" autocomplete="off" />
    <input bind:value={email} placeholder="Email (optional)" type="email" autocomplete="off" />
    <input bind:value={password} placeholder="Password" type="password" autocomplete="new-password" />
    <select bind:value={role}><option value="viewer">Viewer</option><option value="operator">Operator</option><option value="administrator">Administrator</option></select>
    <button on:click={create} disabled={saving || !username.trim() || !password}>{saving ? 'Creating…' : 'Add user'}</button>
  </div>
</section>

<div class="card table-wrap"><table class="table"><thead><tr><th>Username</th><th>Email</th><th>Role</th><th>Status</th><th>Created</th><th>Actions</th></tr></thead><tbody>
{#each users as user}
  <tr>
    {#if editingId === user.id}
      <td>{user.username}</td>
      <td><input bind:value={editEmail} placeholder="Email" /></td>
      <td><select bind:value={editRole}><option value="viewer">Viewer</option><option value="operator">Operator</option><option value="administrator">Administrator</option></select></td>
      <td><label class="check"><input type="checkbox" bind:checked={editEnabled} /> Enabled</label></td>
      <td><input bind:value={editPassword} type="password" placeholder="New password" autocomplete="new-password" /></td>
      <td class="actions"><button on:click={() => saveEdit(user)}>Save</button><button class="ghost" on:click={cancelEdit}>Cancel</button></td>
    {:else}
      <td>{user.username}</td>
      <td class="muted">{user.email ?? '-'}</td>
      <td><span class="pill">{user.role}</span></td>
      <td>{user.enabled ? 'Enabled' : 'Disabled'}</td>
      <td class="muted">{new Date(user.created_at).toLocaleDateString()}</td>
      <td class="actions"><button class="ghost" on:click={() => startEdit(user)}>Edit</button><button class="danger" on:click={() => remove(user)}>Delete</button></td>
    {/if}
  </tr>
{/each}
</tbody></table></div>
{/if}

<style>
  .form { margin: 24px 0; }
  .form-row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .form-row input, .form-row select { background: #0b1017; color: #e7edf5; border: 1px solid #355164; border-radius: 6px; padding: 10px; }
  .form-row input { flex: 1; min-width: 150px; }
  .form-row button { border: 0; border-radius: 6px; padding: 10px 14px; background: #74e0b4; color: #0b1017; font-weight: 700; }
  .table-wrap { overflow-x: auto; }
  td input, td select { background: #0b1017; color: #e7edf5; border: 1px solid #355164; border-radius: 6px; padding: 8px; }
  .check { display: flex; align-items: center; gap: 6px; color: #8293a4; font-size: .85rem; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .actions button { border: 1px solid #355164; border-radius: 6px; background: #162532; color: #dce8f1; padding: 7px 10px; font-size: .8rem; }
  .actions button.ghost { background: transparent; }
  .actions button.danger { border-color: #6d2a35; color: #f0788a; }
</style>
