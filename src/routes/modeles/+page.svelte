<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { engines, dotClass, localKey, type ServerEngine } from "$lib/engines.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Badge, badgeVariants } from "$lib/components/ui/badge";
  import { AlertDialog, AlertDialogTrigger, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from "$lib/components/ui/alert-dialog";
  import { Separator } from "$lib/components/ui/separator";
  import { Switch } from "$lib/components/ui/switch";
  import { Toaster } from "$lib/components/ui/sonner";
  import { ChevronDown, ChevronUp, Plus, RotateCcw, Edit, Trash2, Check, X, Wifi, WifiOff, Cpu, Zap, Loader2, Monitor, Globe, Apple } from "lucide-svelte";
  import { cn } from "$lib/utils";

  const empty = { name: "", url: "http://", apiKey: "", model: "", language: "", insecure: false };

  let form = $state({ ...empty });
  let editingId = $state<string | null>(null);
  let showForm = $state(false);
  let pendingDelete = $state<string | null>(null);
  let error = $state("");

  let importing = $state(false);
  let importMode = $state<"copy" | "link">("copy");
  let importError = $state("");
  let pendingRemove = $state<string | null>(null);

  // Toast state
  let toastError = $state<string | null>(null);

  function showToast(msg: string) {
    toastError = msg;
    setTimeout(() => { toastError = null; }, 5000);
  }

  async function importModel() {
    importError = "";
    const dir = await open({
      directory: true,
      multiple: false,
      title: "Choisis le dossier exporté (model.onnx, vocab.json, meta.json)",
    });
    if (typeof dir !== "string") return;
    importing = true;
    try {
      await engines.importModel(dir, importMode);
    } catch (e) {
      importError = String(e);
      showToast("Erreur lors de l'import : " + String(e));
    } finally {
      importing = false;
    }
  }

  async function removeLocal(folder: string) {
    if (pendingRemove !== folder) {
      pendingRemove = folder;
      return;
    }
    pendingRemove = null;
    try {
      await engines.removeLocal(folder);
    } catch (e) {
      importError = String(e);
      showToast("Erreur : " + String(e));
    }
  }

  function openAdd() {
    form = { ...empty };
    editingId = null;
    error = "";
    showForm = true;
  }

  function openEdit(s: ServerEngine) {
    form = {
      name: s.name,
      url: s.url,
      apiKey: s.apiKey,
      model: s.model,
      language: s.language,
      insecure: s.insecure,
    };
    editingId = s.id;
    error = "";
    showForm = true;
  }

  function submit() {
    if (!form.name.trim()) return (error = "Donne un nom au serveur.");
    if (!/^https?:\/\/.+/.test(form.url.trim())) return (error = "L'URL doit commencer par http:// ou https://");
    const data = { ...form, name: form.name.trim(), url: form.url.trim() };
    if (editingId) engines.update(editingId, data);
    else engines.add(data);
    showForm = false;
  }

  function isLocal(url: string) {
    try {
      return ["localhost", "127.0.0.1", "[::1]"].includes(new URL(url).hostname);
    } catch {
      return false;
    }
  }

  function remove(id: string) {
    if (pendingDelete === id) {
      engines.remove(id);
      pendingDelete = null;
    } else {
      pendingDelete = id;
    }
  }

  const statusColors = {
    ready: "bg-green-500",
    loading: "bg-yellow-500",
    error: "bg-red-500",
    offline: "bg-gray-400",
    unknown: "bg-gray-400",
  } as const;
</script>

<Toaster />

{#if toastError}
  <Toaster.Toast variant="destructive" onDismiss={() => toastError = null}>
    {toastError}
  </Toaster.Toast>
{/if}

<div class="mx-auto max-w-2xl px-8 py-14">
  <h1 class="text-2xl font-semibold tracking-tight">Modèles</h1>

  <section class="mt-8 space-y-6">
    <div class="flex items-center justify-between gap-4">
      <div>
        <h2 class="text-sm font-medium">Moteur local</h2>
        <p class="mt-1 text-xs text-muted-foreground">Modèles ONNX exécutés directement dans l'appli, sans serveur.</p>
      </div>
      <Button variant="outline" size="sm" onclick={() => engines.refreshLocals()}>
        <RotateCcw class="mr-2 h-3 w-3" />
        Rafraîchir
      </Button>
    </div>

    <div class="flex flex-wrap items-center gap-3">
      <Button onclick={importModel} disabled={importing}>
        {#if importing}
          <span class="flex items-center gap-2"><Loader2 class="h-3 w-3 animate-spin" />Import en cours...</span>
        {:else}
          <span class="flex items-center gap-2"><Plus class="h-3 w-3" />Importer un modèle</span>
        {/if}
      </Button>

      <div class="inline-flex gap-1 rounded-lg border border-border bg-card p-1 text-xs" role="group" aria-label="Mode d'import">
        <Button
          variant={importMode === "copy" ? "default" : "ghost"}
          size="sm"
          onclick={() => (importMode = "copy")}
        >
          Copier
        </Button>
        <Button
          variant={importMode === "link" ? "default" : "ghost"}
          size="sm"
          onclick={() => (importMode = "link")}
        >
          Lier
        </Button>
      </div>
    </div>
    <p class="mt-2 text-xs text-muted-foreground">
      {importMode === "copy"
        ? "Copier : le modèle est copié dans l'appli (plusieurs Go, patiente pendant l'import)."
        : "Lier : aucune copie, mais le dossier d'origine doit rester en place (disque monté)."}
    </p>
    {#if importError}<p class="mt-2 text-sm text-destructive">{importError}</p>{/if}

    <div class="flex flex-wrap items-center gap-3">
      <span class="text-xs text-muted-foreground">Calcul</span>
      <div class="inline-flex items-center gap-2 rounded-lg border border-border bg-card p-1">
        <Button
          variant={engines.device === "auto" ? "default" : "ghost"}
          size="sm"
          onclick={() => engines.setDevice("auto")}
        >
          <Zap class="mr-1 h-3 w-3" />
          Auto
        </Button>
        {#if engines.caps.cuda}
          <Button
            variant={engines.device === "cuda" ? "default" : "ghost"}
            size="sm"
            onclick={() => engines.setDevice("cuda")}
          >
            <Cpu class="mr-1 h-3 w-3 text-green-500" />
            CUDA
          </Button>
        {/if}
        {#if engines.caps.directml}
          <Button
            variant={engines.device === "directml" ? "default" : "ghost"}
            size="sm"
            onclick={() => engines.setDevice("directml")}
          >
            <Monitor class="mr-1 h-3 w-3 text-blue-500" />
            DirectML
          </Button>
        {/if}
        {#if engines.caps.webgpu}
          <Button
            variant={engines.device === "webgpu" ? "default" : "ghost"}
            size="sm"
            onclick={() => engines.setDevice("webgpu")}
          >
            <Globe class="mr-1 h-3 w-3 text-purple-500" />
            WebGPU
          </Button>
        {/if}
        {#if engines.caps.coreml}
          <Button
            variant={engines.device === "coreml" ? "default" : "ghost"}
            size="sm"
            onclick={() => engines.setDevice("coreml")}
          >
            <Apple class="mr-1 h-3 w-3" />
            CoreML
          </Button>
        {/if}
        <Button
          variant={engines.device === "cpu" ? "default" : "ghost"}
          size="sm"
          onclick={() => engines.setDevice("cpu")}
        >
          <Cpu class="mr-1 h-3 w-3" />
          CPU
        </Button>
      </div>
    </div>
    <p class="mt-2 text-xs text-muted-foreground">
      Auto : utilise le meilleur périphérique disponible, sinon le processeur. Le CPU fonctionne sur tous les ordinateurs.
      {#if engines.caps.platform === "windows"}
        CUDA (build dédié <span class="font-mono">--features gpu</span> uniquement) et DirectML (natif Windows) si disponibles.
      {:else if engines.caps.platform === "linux"}
        CUDA (build dédié <span class="font-mono">--features gpu</span> uniquement) et WebGPU (Vulkan, natif Linux) si disponibles.
      {:else if engines.caps.platform === "macos"}
        CoreML (Neural Engine/GPU Apple, natif macOS) si disponible. Pas de CUDA sur Mac.
      {/if}
      {#if !engines.caps.cuda && !engines.caps.directml && !engines.caps.webgpu && !engines.caps.coreml}
        Ce build ne propose que le CPU (versions GPU : builds dédiés).
      {/if}
    </p>

    <div class="space-y-3">
      {#each engines.locals as m (m.id)}
        {@const key = localKey(m.id)}
        {@const st = engines.status[key] ?? { state: "unknown", message: "Non chargé" }}
        {@const active = engines.activeId === key}
        <Card class={cn(active ? "border-primary" : "border-border", "bg-card")}>
          <CardContent class="p-4">
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <Badge
                    variant="outline"
                    class={cn("gap-1.5 px-2 py-0.5", statusColors[st.state] ?? "bg-gray-400")}
                  >
                    <span class={`size-1.5 rounded-full ${statusColors[st.state] ?? "bg-gray-400"}`} />
                    <span class="text-xs capitalize">{st.state}</span>
                  </Badge>
                  <span class="truncate text-sm font-medium">{m.name}</span>
                  {#if active}
                    <Badge variant="secondary" class="text-xs">Actif</Badge>
                  {/if}
                </div>
                <div class="mt-1 text-xs text-muted-foreground">{m.sizeMb} Mo · hors ligne, l'audio reste sur cet ordinateur</div>
                <div class="mt-1 text-xs text-muted-foreground">{st.message}</div>
              </div>
              <div class="flex shrink-0 items-center gap-2">
                {#if !active}
                  <Button
                    variant="outline"
                    size="sm"
                    onclick={() => engines.setActive(key)}
                    class="h-7 px-2.5"
                  >
                    Utiliser
                  </Button>
                {:else if st.state === "offline"}
                  <Button variant="outline" size="sm" onclick={() => engines.check(key)} class="h-7 px-2.5">
                    <RotateCcw class="mr-1 h-3 w-3" />
                    Réessayer
                  </Button>
                {/if}
                <AlertDialog>
                  <AlertDialogTrigger asChild>
                    <Button
                      variant="ghost"
                      size="sm"
                      class={cn("h-7 px-2.5", pendingRemove === m.id && "text-destructive")}
                      onclick={() => removeLocal(m.id)}
                      onblur={() => (pendingRemove = null)}
                    >
                      {pendingRemove === m.id ? "Confirmer ?" : "Retirer"}
                    </Button>
                  </AlertDialogTrigger>
                  <AlertDialogContent>
                    <AlertDialogHeader>
                      <AlertDialogTitle>Retirer le modèle ?</AlertDialogTitle>
                      <AlertDialogDescription>
                        Cette action va supprimer le modèle <strong>{m.name}</strong> de l'application.
                        {importMode === "link" && " Le dossier d'origine ne sera pas affecté."}
                      </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                      <AlertDialogCancel>Annuler</AlertDialogCancel>
                      <AlertDialogAction onClick={() => { pendingRemove = m.id; removeLocal(m.id); }}>Retirer</AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              </div>
            </div>
          </CardContent>
        </Card>
      {:else}
        <Card class="border-dashed border-border bg-card/50">
          <CardContent class="p-6 text-center">
            <p class="text-sm text-muted-foreground">Aucun modèle local installé.</p>
            <p class="mt-2 text-sm text-muted-foreground">
              Exporte-en un avec <code class="bg-secondary text-secondary-foreground px-1 rounded">export_onnx.py</code>, puis clique sur « Importer un modèle » et choisis le dossier
              <code class="bg-secondary text-secondary-foreground px-1 rounded">export/mms-fmp</code>.
            </p>
            <p class="mt-2 break-all text-xs text-muted-foreground">Dossier des modèles : {engines.modelsDir || "…"}</p>
          </CardContent>
        </Card>
      {/each}
    </div>
  </section>

  <Separator class="my-8" />

  <div class="flex items-start justify-between gap-4">
    <div>
      <h2 class="text-sm font-medium">Serveurs</h2>
      <p class="mt-1 text-xs text-muted-foreground">
        Transcription via une API compatible OpenAI (<code class="bg-secondary text-secondary-foreground px-1 rounded">/v1/audio/transcriptions</code>).
      </p>
    </div>
    <Button onclick={openAdd}>
      <Plus class="mr-2 h-3 w-3" />
      Ajouter un serveur
    </Button>
  </div>

  {#if showForm}
    <Card class="mt-6">
      <CardHeader>
        <CardTitle>{editingId ? "Modifier le serveur" : "Nouveau serveur"}</CardTitle>
      </CardHeader>
      <CardContent class="space-y-4">
        <form onsubmit={(e) => { e.preventDefault(); submit(); }}>
          <div class="space-y-4">
            <div class="space-y-1.5">
              <Label htmlFor="name" class="text-xs text-muted-foreground">Nom</Label>
              <Input id="name" bind:value={form.name} placeholder="Mon serveur" class="mt-1" />
            </div>

            <div class="space-y-1.5">
              <Label htmlFor="url" class="text-xs text-muted-foreground">URL</Label>
              <Input id="url" type="url" bind:value={form.url} placeholder="http://127.0.0.1:8000" class="mt-1" />
            </div>

            <div class="space-y-1.5">
              <Label htmlFor="apiKey" class="text-xs text-muted-foreground">Clé API (optionnelle)</Label>
              <Input id="apiKey" type="password" bind:value={form.apiKey} autocomplete="off" class="mt-1" />
            </div>

            <div class="grid grid-cols-2 gap-4">
              <div class="space-y-1.5">
                <Label htmlFor="model" class="text-xs text-muted-foreground">Modèle (optionnel)</Label>
                <Input id="model" bind:value={form.model} placeholder="default" class="mt-1" />
              </div>
              <div class="space-y-1.5">
                <Label htmlFor="language" class="text-xs text-muted-foreground">Langue (optionnelle)</Label>
                <Input id="language" bind:value={form.language} placeholder="fmp" class="mt-1" />
              </div>
            </div>

            <details class="text-xs text-muted-foreground">
              <summary class="cursor-pointer select-none flex items-center gap-1">
                Avancé <ChevronDown class="h-3 w-3" />
              </summary>
              <div class="mt-3 flex items-start gap-2">
                <input type="checkbox" bind:checked={form.insecure} id="insecure" class="mt-0.5" />
                <label for="insecure" class="text-xs">Accepter les certificats auto-signés (déconseillé hors réseau de confiance)</label>
              </div>
            </details>

            {#if error}<p class="text-sm text-destructive">{error}</p>{/if}

            <div class="flex justify-end gap-2 pt-2 border-t">
              <Button type="button" variant="ghost" onclick={() => (showForm = false)}>Annuler</Button>
              <Button type="submit">Enregistrer</Button>
            </div>
          </div>
        </form>
      </CardContent>
    </Card>
  {/if}

  <div class="mt-8 space-y-3">
    {#each engines.servers as s (s.id)}
      {@const st = engines.status[s.id] ?? { state: "unknown", message: "Non testé" }}
      {@const active = engines.activeId === s.id}
      <Card class={cn(active ? "border-primary" : "border-border", "bg-card")}>
        <CardContent class="p-4">
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <Badge
                  variant="outline"
                  class={cn("gap-1.5 px-2 py-0.5", statusColors[st.state] ?? "bg-gray-400")}
                >
                  <span class={`size-1.5 rounded-full ${statusColors[st.state] ?? "bg-gray-400"}`}></span>
                  <span class="text-xs capitalize">{st.state}</span>
                </Badge>
                <span class="truncate text-sm font-medium">{s.name}</span>
                {#if active}
                  <Badge variant="secondary" class="text-xs">Actif</Badge>
                {/if}
              </div>
              <div class="mt-1 truncate text-xs text-muted-foreground">{s.url}</div>
              <div class="mt-1 text-xs text-muted-foreground">{st.message}</div>
            </div>

            {#if !active}
              <Button variant="outline" size="sm" onclick={() => engines.setActive(s.id)} class="shrink-0 h-7 px-2.5">
                Utiliser
              </Button>
            {/if}
          </div>

          {#if !isLocal(s.url)}
            <div class="mt-3 rounded-lg bg-amber-500/10 border border-amber-500/20 p-3">
              <div class="flex items-start gap-2">
                <WifiOff class="h-4 w-4 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" />
                <div class="text-xs text-amber-600 dark:text-amber-400">
                  <p>Serveur distant : l'audio enregistré lui est envoyé.</p>
                  {#if s.url.trim().startsWith("http://")}
                    <p class="mt-1">⚠ Connexion non chiffrée (http).</p>
                  {/if}
                </div>
              </div>
            </div>
          {/if}

          <div class="mt-3 flex items-center gap-2">
            <Button variant="outline" size="sm" onclick={() => engines.test(s.id)} class="h-7 px-2.5">
              <RotateCcw class="mr-1 h-3 w-3" />
              Tester
            </Button>
            <Button variant="outline" size="sm" onclick={() => openEdit(s)} class="h-7 px-2.5">
              <Edit class="mr-1 h-3 w-3" />
              Modifier
            </Button>
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  class={cn("h-7 px-2.5", pendingDelete === s.id && "text-destructive")}
                  onclick={() => remove(s.id)}
                  onblur={() => (pendingDelete = null)}
                >
                  {pendingDelete === s.id ? "Confirmer ?" : "Supprimer"}
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Supprimer le serveur ?</AlertDialogTitle>
                  <AlertDialogDescription>
                    Cette action va supprimer définitivement le serveur <strong>{s.name}</strong>.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>Annuler</AlertDialogCancel>
                  <AlertDialogAction onClick={() => { pendingDelete = s.id; remove(s.id); }}>Supprimer</AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </div>
        </CardContent>
      </Card>
    {:else}
      <Card class="border-dashed border-border bg-card/50">
        <CardContent class="p-10 text-center">
          <p class="text-sm text-muted-foreground">Aucun serveur configuré.</p>
        </CardContent>
      </Card>
    {/each}
  </div>
</div>