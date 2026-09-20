<script lang="ts">
  import { onMount } from 'svelte';
  type User = { id:string; username:string; email?:string; role:string; enabled:boolean; created_at:string };
  let users: User[] = []; let message = '';
  onMount(async () => { const response = await fetch('/api/users'); if (response.ok) users = await response.json(); else message = await response.text(); });
</script>
<svelte:head><title>Users · Objexel</title></svelte:head>
<p class="eyebrow">Access control</p><h1>Users</h1><p class="muted">Administrator-only local accounts and roles.</p>{#if message}<div class="empty">{message}</div>{:else}<div class="card"><table class="table"><thead><tr><th>Username</th><th>Email</th><th>Role</th><th>Status</th><th>Created</th></tr></thead><tbody>{#each users as user}<tr><td>{user.username}</td><td class="muted">{user.email ?? '—'}</td><td><span class="pill">{user.role}</span></td><td>{user.enabled ? 'Enabled' : 'Disabled'}</td><td class="muted">{new Date(user.created_at).toLocaleDateString()}</td></tr>{/each}</tbody></table></div>{/if}
