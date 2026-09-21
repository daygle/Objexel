<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, apiError, setCsrf, sessionActive, SESSION_NOT_STORED_MESSAGE } from '$lib/api';
  let username = ''; let password = ''; let message = ''; let busy = false;
  async function submit() { busy = true; message = ''; const response = await api('/api/auth/login', { method:'POST', json:{ username, password }, redirectOnUnauthorized:false }); if (response.ok) { const result = await response.json(); setCsrf(result.csrf_token); if (await sessionActive()) await goto('/observations'); else message = SESSION_NOT_STORED_MESSAGE; } else message = await apiError(response); busy = false; }
</script>
<svelte:head><title>Sign in · Objexel</title></svelte:head>
<div class="auth card"><p class="eyebrow">Local access</p><h1>Sign in to Objexel</h1><p class="muted">Use your local administrator, operator, or viewer account.</p><form on:submit|preventDefault={submit}><label>Username<input bind:value={username} autocomplete="username" required /></label><label>Password<input bind:value={password} type="password" autocomplete="current-password" required /></label>{#if message}<p class="error">{message}</p>{/if}<button disabled={busy}>{busy ? 'Signing in…' : 'Sign in'}</button></form><a href="/setup">First run setup</a></div>
<style>.auth{max-width:440px;margin:8vh auto}.auth h1{margin-bottom:8px}form{display:grid;gap:16px;margin:28px 0}label{display:grid;gap:7px;color:#8293a4;font-size:.8rem}input{background:#0b1017;border:1px solid #355164;border-radius:6px;padding:11px;color:#e7edf5}button{background:#74e0b4;color:#0b1017;border:0;border-radius:6px;padding:11px;font-weight:700}.error{color:#ff9b9b}a{color:#74e0b4}</style>
