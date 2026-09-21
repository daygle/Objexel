<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { api, apiError, setCsrf, sessionActive, SESSION_NOT_STORED_MESSAGE } from '$lib/api';
  let username = ''; let email = ''; let password = ''; let message = ''; let busy = false; let ready = false;
  onMount(async () => { const response = await api('/api/auth/setup', { redirectOnUnauthorized:false }); if (response.ok) { const data = await response.json(); if (!data.setup_required) { await goto('/login'); return; } } ready = true; });
  async function submit() { busy = true; const response = await api('/api/auth/setup', { method:'POST', json:{ username, email: email || null, password, role:'administrator' }, redirectOnUnauthorized:false }); if (response.ok) { const result = await response.json(); setCsrf(result.csrf_token); if (await sessionActive()) await goto('/observations'); else message = SESSION_NOT_STORED_MESSAGE; } else message = await apiError(response); busy = false; }
</script>
<svelte:head><title>Initial setup · Objexel</title></svelte:head>
{#if ready}
<div class="auth card"><p class="eyebrow">First run</p><h1>Create administrator</h1><p class="muted">This wizard is available only until the first user is created.</p><form on:submit|preventDefault={submit}><label>Username<input bind:value={username} required /></label><label>Email<input bind:value={email} type="email" /></label><label>Password<input bind:value={password} type="password" minlength="12" required /></label>{#if message}<p class="error">{message}</p>{/if}<button disabled={busy}>{busy ? 'Creating…' : 'Create administrator'}</button></form><a href="/login">Already configured? Sign in</a></div>
{/if}
<style>.auth{max-width:440px;margin:8vh auto}.auth h1{margin-bottom:8px}form{display:grid;gap:16px;margin:28px 0}label{display:grid;gap:7px;color:#8293a4;font-size:.8rem}input{background:#0b1017;border:1px solid #355164;border-radius:6px;padding:11px;color:#e7edf5}button{background:#74e0b4;color:#0b1017;border:0;border-radius:6px;padding:11px;font-weight:700}.error{color:#ff9b9b}a{color:#74e0b4}</style>
