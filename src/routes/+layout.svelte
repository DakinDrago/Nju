<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { theme } from "$lib/theme.svelte";
  import { engines } from "$lib/engines.svelte";

  let { children } = $props();

  onMount(() => {
    theme.init();
    engines.init();
  });
</script>

<div class="flex h-screen overflow-hidden">
  <Sidebar />
  <main class="flex-1 overflow-y-auto">
    {@render children()}
  </main>
</div>