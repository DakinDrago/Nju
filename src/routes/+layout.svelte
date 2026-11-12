<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { theme } from "$lib/theme.svelte";
  import { engines } from "$lib/engines.svelte";
  import { Toaster } from "$lib/components/ui/sonner";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/components/ui/select";
  import { Button } from "$lib/components/ui/button";
  import { microphone } from "$lib/stores";
  import { settings } from "$lib/settings.svelte";
  import { updater } from "$lib/updater.svelte";
  import { whatsNew } from "$lib/whats-new.svelte";
  import WhatsNewDialog from "$lib/components/WhatsNewDialog.svelte";
  import { toast } from "svelte-sonner";
  import { Mic, Loader2, RefreshCw } from "lucide-svelte";

  let { children } = $props();

  const micDevices = microphone.devices;
  const micLoading = microphone.isLoading;
  const micSelectedDevice = microphone.selectedDevice;

  let updateCheckStarted = false;

  // Vérifie les mises à jour une fois l'éventuelle fenêtre « Nouveautés »
  // refermée, pour ne pas afficher un toast derrière le dialogue.
  $effect(() => {
    if (!whatsNew.ready || whatsNew.open || updateCheckStarted) return;
    updateCheckStarted = true;
    void updater.checkForUpdates(true).then((found) => {
      if (found) toast.success(`Mise à jour ${updater.availableVersion} disponible (voir Paramètres)`);
    });
  });

  onMount(() => {
    // En premier : l'absence de données `nju-*` distingue une installation
    // neuve d'une mise à jour (voir whats-new.svelte.ts).
    void whatsNew.init();
    theme.init();
    engines.init();
    microphone.loadDevices();
    const stopMicWatch = microphone.startWatching();
    void settings.init();

    // Production : neutralise les raccourcis de rechargement de la WebView.
    // (Windows a en plus un blocage natif WebView2 ; ceci couvre Linux/macOS.)
    // Ne touche ni au copier/coller ni à la saisie.
    const blockReload = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      const mod = e.ctrlKey || e.metaKey;
      if (key === "f5" || (mod && key === "r")) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    if (import.meta.env.PROD) {
      window.addEventListener("keydown", blockReload, { capture: true });
    }

    return () => {
      stopMicWatch();
      window.removeEventListener("keydown", blockReload, { capture: true });
    };
  });
</script>

<Toaster />
<WhatsNewDialog />

<div class="flex h-screen overflow-hidden">
  <Sidebar />
  <main class="flex min-h-0 flex-1 flex-col overflow-hidden">
    <header class="flex items-center justify-between px-4 py-3 border-b border-border bg-background/95 backdrop-blur-sm sticky top-0 z-10">
      <div class="flex items-center gap-4">
        <h1 class="text-lg font-semibold tracking-tight">Njū'</h1>
      </div>
      <div class="flex items-center gap-3">
        <div class="flex items-center gap-1">
          <div class="relative">
            {#if $micLoading}
              <div class="flex h-8 w-[220px] items-center gap-2 text-sm text-muted-foreground">
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
                onOpenChange={(open) => {
                  if (open) void microphone.loadDevices(true);
                }}
                disabled={$micDevices.length === 0}
              >
                <SelectTrigger class="h-8 w-[220px] text-sm" aria-label="Sélectionner le microphone">
                  <span class="flex min-w-0 flex-1 items-center gap-2">
                    <Mic class="h-4 w-4 shrink-0 text-muted-foreground" />
                    <span
                      class="truncate"
                      title={$micSelectedDevice?.label ?? "Aucun microphone détecté"}
                    >
                      {$micSelectedDevice?.label ?? "Aucun microphone détecté"}
                    </span>
                  </span>
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
                        <div class="flex min-w-0 items-center gap-2">
                          <Mic class="h-4 w-4 shrink-0 text-muted-foreground" />
                          <span
                            class="max-w-[180px] truncate"
                            title={device.label || `Microphone ${device.deviceId.slice(0, 8)}...`}
                          >
                            {device.label || `Microphone ${device.deviceId.slice(0, 8)}...`}
                          </span>
                        </div>
                      </SelectItem>
                    {/each}
                  {/if}
                </SelectContent>
              </Select>
            {/if}
          </div>
          <Button
            variant="ghost"
            size="icon"
            class="h-8 w-8 shrink-0"
            title="Rechercher les microphones"
            aria-label="Rechercher les microphones"
            onclick={() => void microphone.loadDevices()}
            disabled={$micLoading}
          >
            {#if $micLoading}
              <Loader2 class="h-4 w-4 animate-spin" />
            {:else}
              <RefreshCw class="h-4 w-4" />
            {/if}
          </Button>
        </div>
      </div>
    </header>
    <div class="min-h-0 flex-1 overflow-y-auto">
      {@render children()}
    </div>
  </main>
</div>
