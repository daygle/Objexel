<script lang="ts">
  import '../app.css';
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, clearCsrf } from '$lib/api';
  type UpdateInfo = { current_version:string; latest_version?:string; update_available:boolean; release_url?:string; notes?:string };
  let update: UpdateInfo | null = null;
  let signedIn = false;
  let openDropdown: string | null = null;

  async function loadUpdate() { const response = await api('/api/updates', { redirectOnUnauthorized:false }); if (response.ok) update = await response.json(); }
  async function loadSession() { const response = await api('/api/auth/me', { redirectOnUnauthorized:false }); signedIn = response.ok; }
  async function signOut() { await api('/api/auth/logout', { method:'POST', redirectOnUnauthorized:false }); clearCsrf(); signedIn = false; await goto('/login'); }

  function toggleDropdown(name: string) { openDropdown = openDropdown === name ? null : name; }
  function closeDropdowns() { openDropdown = null; }
  function handleClickOutside(event: MouseEvent) { if (!(event.target as HTMLElement).closest('.nav-item')) openDropdown = null; }

  onMount(() => { loadUpdate(); loadSession(); document.addEventListener('click', handleClickOutside); });
  onDestroy(() => { if (typeof document !== 'undefined') document.removeEventListener('click', handleClickOutside); });
</script>

<nav>
  <a class="brand" href="/observations">OBJEXEL <span>EDGE INTELLIGENCE</span></a>
  <div class="nav-links">
    {#if signedIn}
      <a href="/observations">Observations</a>
      <a href="/cameras">Cameras</a>

      <div class="nav-item">
        <button class="nav-btn" on:click|stopPropagation={() => toggleDropdown('detection')}>Detection</button>
        {#if openDropdown === 'detection'}
          <div class="dropdown">
            <a href="/detections" on:click={closeDropdowns}>Detections</a>
            <a href="/tracks" on:click={closeDropdowns}>Tracks</a>
            <a href="/behaviours" on:click={closeDropdowns}>Behaviours</a>
            <a href="/identities" on:click={closeDropdowns}>Identities</a>
          </div>
        {/if}
      </div>

      <div class="nav-item">
        <button class="nav-btn" on:click|stopPropagation={() => toggleDropdown('intelligence')}>Intelligence</button>
        {#if openDropdown === 'intelligence'}
          <div class="dropdown">
            <a href="/analytics" on:click={closeDropdowns}>Analytics</a>
            <a href="/intelligence" on:click={closeDropdowns}>Intelligence</a>
            <a href="/rules" on:click={closeDropdowns}>Rules</a>
            <a href="/events" on:click={closeDropdowns}>Events</a>
            <a href="/zones" on:click={closeDropdowns}>Zones</a>
          </div>
        {/if}
      </div>

      <div class="nav-item">
        <button class="nav-btn" on:click|stopPropagation={() => toggleDropdown('models')}>Models</button>
        {#if openDropdown === 'models'}
          <div class="dropdown">
            <a href="/models" on:click={closeDropdowns}>Models</a>
            <a href="/pipelines" on:click={closeDropdowns}>AI Pipelines</a>
          </div>
        {/if}
      </div>

      <div class="nav-item">
        <button class="nav-btn" on:click|stopPropagation={() => toggleDropdown('system')}>System</button>
        {#if openDropdown === 'system'}
          <div class="dropdown">
            <a href="/recordings" on:click={closeDropdowns}>Recordings</a>
            <a href="/search" on:click={closeDropdowns}>Search</a>
            <a href="/notifications" on:click={closeDropdowns}>Notifications</a>
            <a href="/users" on:click={closeDropdowns}>Users</a>
            <a href="/settings" on:click={closeDropdowns}>Settings</a>
          </div>
        {/if}
      </div>

      <a href="#logout" on:click|preventDefault={signOut}>Sign out</a>
    {:else}
      <a href="/login">Sign in</a>
    {/if}
  </div>
</nav>

{#if signedIn && update?.update_available}<aside class="update"><strong>Objexel {update.latest_version} is available.</strong><span>Updates are operator-controlled: back up PostgreSQL and media before upgrading.</span>{#if update.release_url}<a href={update.release_url} target="_blank" rel="noreferrer">Read release notes</a>{/if}</aside>{/if}
<main><slot /></main>

<style>
  .nav-links { display: flex; align-items: center; gap: 4px; }
  .nav-item { position: relative; }
  .nav-btn {
    background: none; border: none; color: #8fa2b5; font-size: .9rem;
    font-family: inherit; cursor: pointer; padding: 8px 12px; border-radius: 6px;
    transition: color .15s, background .15s;
  }
  .nav-btn:hover { color: #74e0b4; background: rgba(116,224,180,.06); }
  .dropdown {
    position: absolute; top: 100%; left: 0; margin-top: 4px;
    background: #111b25; border: 1px solid #223342; border-radius: 8px;
    padding: 6px 0; min-width: 170px; z-index: 100;
    box-shadow: 0 12px 32px rgba(0,0,0,.4);
  }
  .dropdown a {
    display: block; padding: 8px 16px; margin: 0; color: #b0c0d0;
    font-size: .85rem; border-radius: 0; white-space: nowrap;
  }
  .dropdown a:hover { background: rgba(116,224,180,.08); color: #74e0b4; }
  .update { display: flex; gap: 14px; align-items: center; flex-wrap: wrap; padding: 12px 6vw; background: #3b2d1d; color: #f0c56d; border-bottom: 1px solid #72552b; font-size: .85rem; }
  .update span { color: #f6dfac; }
  .update a { color: #fff1c9; }
  @media(max-width: 900px) {
    nav { height: auto; padding: 18px 6vw; align-items: flex-start; gap: 12px; flex-direction: column; }
    .nav-links { flex-wrap: wrap; gap: 4px; }
    .dropdown { position: static; box-shadow: none; border: 1px solid #223342; margin-top: 4px; }
  }
</style>
