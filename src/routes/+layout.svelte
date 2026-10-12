<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { theme } from "$lib/theme.svelte";
  import { engines } from "$lib/engines.svelte";
  import { Toaster } from "$lib/components/ui/sonner";
  import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "$lib/components/ui/select";
  import { microphone } from "$lib/stores";
  import { settings } from "$lib/settings.svelte";
  import { Mic, Loader2 } from "lucide-svelte";

  let { children } = $props();

  const micDevices = microphone.devices;
  const micLoading = microphone.isLoading;
  const micSelectedDevice = microphone.selectedDevice;

  onMount(() => {
    theme.init();
    engines.init();
    microphone.loadDevices();
    void settings.init();
  });
</script>

<Toaster />

<div class="flex h-screen overflow-hidden">
  <Sidebar />
  <main class="flex-1 overflow-y-auto flex flex-col">
    <header class="flex items-center justify-between px-4 py-3 border-b border-border bg-background/95 backdrop-blur-sm sticky top-0 z-10">
      <div class="flex items-center gap-4">
        <h1 class="text-lg font-semibold tracking-tight">Njū'</h1>
      </div>
      <div class="flex items-center gap-3">
        <div class="relative">
          {#if $micLoading}
            <div class="flex h-8 w-[200px] items-center gap-2 text-sm text-muted-foreground">
              <Loader2 class="h-4 w-4 animate-spin" />
              <span>Chargement...</span>
            </div>
          {:else}
            <Select
              type="single"
              value={$microphone ?? ""}
              onValueChange={(val) => {
                if (typeof val === "string" && val) microphone.selectDevice(val);
              }}
              disabled={$micDevices.length === 0}
            >
              <SelectTrigger class="w-[200px] h-8 text-sm" aria-label="Sélectionner le microphone">
                {#if $micSelectedDevice}
                  <div class="flex items-center gap-2">
                    <Mic class="h-4 w-4 text-muted-foreground" />
                    <SelectValue placeholder="Aucun microphone" />
                  </div>
                {:else}
                  <SelectValue placeholder="Aucun microphone détecté" />
                {/if}
              </SelectTrigger>
              <SelectContent position="popper" side="bottom" sideOffset={4}>
                {#if $micDevices.length === 0}
                  <SelectItem value="__none" disabled>
                    <div class="flex items-center gap-2 text-muted-foreground py-2">
                      <span>Aucun microphone détecté</span>
                    </div>
                  </SelectItem>
                {:else}
                  {#each $micDevices as device}
                    <SelectItem value={device.deviceId}>
                      <div class="flex items-center gap-2">
                        <Mic class="h-4 w-4 text-muted-foreground" />
                        <span class="truncate max-w-[200px]">{device.label || `Microphone ${device.deviceId.slice(0, 8)}...`}</span>
                      </div>
                    </SelectItem>
                  {/each}
                {/if}
              </SelectContent>
            </Select>
          {/if}
        </div>
      </div>
    </header>
    <div class="flex-1 overflow-y-auto">
      {@render children()}
    </div>
  </main>
</div>
