<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';

  onMount(async () => {
    const response = await fetch('/api/auth/me');
    await goto(response.ok ? '/observations' : '/login');
  });
</script>

<p class="muted">Checking Objexel session…</p>
