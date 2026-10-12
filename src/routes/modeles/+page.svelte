<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { engines, dotClass, localKey, type ServerEngine } from "$lib/engines.svelte";

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
    }
  }

  const input =
    "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:border-accent";
  const btn =
    "rounded-md px-2.5 py-1 text-xs text-muted transition-colors hover:bg-border/50 hover:text-foreground";

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
</script>

<div class="mx-auto max-w-2xl px-8 py-14">
  <h1 class="text-2xl font-semibold tracking-tight">Modèles</h1>

  <section class="mt-8">
    <div class="flex items-center justify-between gap-4">
      <div>
        <h2 class="text-sm font-medium">Moteur local</h2>
        <p class="mt-1 text-xs text-muted">Modèles ONNX exécutés directement dans l'appli, sans serveur.</p>
      </div>
      <button class={btn} onclick={() => engines.refreshLocals()}>Rafraîchir</button>
    </div>

    <div class="mt-4 flex flex-wrap items-center gap-3">
      <button
        onclick={importModel}
        disabled={importing}
        class="rounded-lg bg-accent px-3 py-2 text-sm font-medium text-accent-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
      >
        {importing ? "Import en cours..." : "Importer un modèle"}
      </button>

      <div class="inline-flex gap-1 rounded-lg border border-border bg-surface p-1 text-xs">
        <button
          class={`rounded-md px-2.5 py-1 transition-colors ${importMode === "copy" ? "bg-accent text-accent-foreground" : "text-muted hover:text-foreground"}`}
          onclick={() => (importMode = "copy")}
        >
          Copier
        </button>
        <button
          class={`rounded-md px-2.5 py-1 transition-colors ${importMode === "link" ? "bg-accent text-accent-foreground" : "text-muted hover:text-foreground"}`}
          onclick={() => (importMode = "link")}
        >
          Lier
        </button>
      </div>
    </div>
    <p class="mt-2 text-xs text-muted">
      {importMode === "copy"
        ? "Copier : le modèle est copié dans l'appli (plusieurs Go, patiente pendant l'import)."
        : "Lier : aucune copie, mais le dossier d'origine doit rester en place (disque monté)."}
    </p>
    {#if importError}<p class="mt-2 text-sm text-danger">{importError}</p>{/if}

    <div class="mt-5 flex flex-wrap items-center gap-3">
      <span class="text-xs text-muted">Calcul</span>
      <div class="inline-flex gap-1 rounded-lg border border-border bg-surface p-1 text-xs">
        <button
          class={`rounded-md px-2.5 py-1 transition-colors ${engines.device === "auto" ? "bg-accent text-accent-foreground" : "text-muted hover:text-foreground"}`}
          onclick={() => engines.setDevice("auto")}
        >
          Auto
        </button>
        <button
          class={`rounded-md px-2.5 py-1 transition-colors ${engines.device === "cpu" ? "bg-accent text-accent-foreground" : "text-muted hover:text-foreground"}`}
          onclick={() => engines.setDevice("cpu")}
        >
          CPU
        </button>
      </div>
    </div>
    <p class="mt-2 text-xs text-muted">
      Auto : utilise la carte graphique NVIDIA si elle est disponible, sinon le processeur. Le CPU fonctionne sur tous les ordinateurs.
    </p>

    <div class="mt-4 space-y-3">
      {#each engines.locals as m (m.id)}
        {@const key = localKey(m.id)}
        {@const st = engines.status[key] ?? { state: "unknown", message: "Non chargé" }}
        {@const active = engines.activeId === key}
        <div class={`flex items-start justify-between gap-3 rounded-xl border bg-surface p-4 ${active ? "border-accent" : "border-border"}`}>
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <span class={`size-2 shrink-0 rounded-full ${dotClass[st.state]}`}></span>
              <span class="truncate text-sm font-medium">{m.name}</span>
              {#if active}
                <span class="rounded-full bg-accent/10 px-2 py-0.5 text-xs text-accent">Actif</span>
              {/if}
            </div>
            <div class="mt-1 text-xs text-muted">{m.sizeMb} Mo · hors ligne, l'audio reste sur cet ordinateur</div>
            <div class="mt-1 text-xs text-muted">{st.message}</div>
          </div>
          <div class="flex shrink-0 items-center gap-1">
            {#if !active}
              <button
                onclick={() => engines.setActive(key)}
                class="rounded-lg border border-border px-3 py-1.5 text-xs font-medium transition-colors hover:border-accent hover:text-accent"
              >
                Utiliser
              </button>
            {:else if st.state === "offline"}
              <button class={btn} onclick={() => engines.check(key)}>Réessayer</button>
            {/if}
            <button
              class={`${btn} ${pendingRemove === m.id ? "text-danger" : ""}`}
              onclick={() => removeLocal(m.id)}
              onblur={() => (pendingRemove = null)}
            >
              {pendingRemove === m.id ? "Confirmer ?" : "Retirer"}
            </button>
          </div>
        </div>
      {:else}
        <div class="rounded-xl border border-dashed border-border p-6 text-sm text-muted">
          <p>Aucun modèle local installé.</p>
          <p class="mt-2">
            Exporte-en un avec <code>export_onnx.py</code>, puis clique sur « Importer un modèle » et choisis le dossier
            <code>export/mms-fmp</code>.
          </p>
          <p class="mt-2 break-all text-xs">Dossier des modèles : {engines.modelsDir || "…"}</p>
        </div>
      {/each}
    </div>
  </section>

  <div class="mt-12 flex items-start justify-between gap-4">
    <div>
      <h2 class="text-sm font-medium">Serveurs</h2>
      <p class="mt-1 text-xs text-muted">
        Transcription via une API compatible OpenAI (<code>/v1/audio/transcriptions</code>).
      </p>
    </div>
    <button
      onclick={openAdd}
      class="shrink-0 rounded-lg bg-accent px-3 py-2 text-sm font-medium text-accent-foreground transition-opacity hover:opacity-90"
    >
      Ajouter un serveur
    </button>
  </div>

  {#if showForm}
    <form
      class="mt-8 space-y-4 rounded-xl border border-border bg-surface p-5"
      onsubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <h2 class="text-sm font-medium">{editingId ? "Modifier le serveur" : "Nouveau serveur"}</h2>

      <label class="block text-xs text-muted">
        Nom
        <input class={`${input} mt-1`} bind:value={form.name} placeholder="Mon serveur" />
      </label>

      <label class="block text-xs text-muted">
        URL
        <input class={`${input} mt-1`} bind:value={form.url} placeholder="http://127.0.0.1:8000" />
      </label>

      <label class="block text-xs text-muted">
        Clé API (optionnelle)
        <input class={`${input} mt-1`} type="password" bind:value={form.apiKey} autocomplete="off" />
      </label>

      <div class="grid grid-cols-2 gap-4">
        <label class="block text-xs text-muted">
          Modèle (optionnel)
          <input class={`${input} mt-1`} bind:value={form.model} placeholder="default" />
        </label>
        <label class="block text-xs text-muted">
          Langue (optionnelle)
          <input class={`${input} mt-1`} bind:value={form.language} placeholder="fmp" />
        </label>
      </div>

      <details class="text-xs text-muted">
        <summary class="cursor-pointer select-none">Avancé</summary>
        <label class="mt-3 flex items-start gap-2">
          <input type="checkbox" bind:checked={form.insecure} class="mt-0.5" />
          <span>Accepter les certificats auto-signés (déconseillé hors réseau de confiance)</span>
        </label>
      </details>

      {#if error}<p class="text-sm text-danger">{error}</p>{/if}

      <div class="flex justify-end gap-2">
        <button type="button" class={btn} onclick={() => (showForm = false)}>Annuler</button>
        <button
          type="submit"
          class="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-accent-foreground hover:opacity-90"
        >
          Enregistrer
        </button>
      </div>
    </form>
  {/if}

  <div class="mt-8 space-y-3">
    {#each engines.servers as s (s.id)}
      {@const st = engines.status[s.id] ?? { state: "unknown", message: "Non testé" }}
      {@const active = engines.activeId === s.id}
      <div class={`rounded-xl border bg-surface p-4 ${active ? "border-accent" : "border-border"}`}>
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <span class={`size-2 shrink-0 rounded-full ${dotClass[st.state]}`}></span>
              <span class="truncate text-sm font-medium">{s.name}</span>
              {#if active}
                <span class="rounded-full bg-accent/10 px-2 py-0.5 text-xs text-accent">Actif</span>
              {/if}
            </div>
            <div class="mt-1 truncate text-xs text-muted">{s.url}</div>
            <div class="mt-1 text-xs text-muted">{st.message}</div>
          </div>

          {#if !active}
            <button
              onclick={() => engines.setActive(s.id)}
              class="shrink-0 rounded-lg border border-border px-3 py-1.5 text-xs font-medium transition-colors hover:border-accent hover:text-accent"
            >
              Utiliser
            </button>
          {/if}
        </div>

        {#if !isLocal(s.url)}
          <p class="mt-3 rounded-lg bg-amber-500/10 px-3 py-2 text-xs text-amber-600 dark:text-amber-400">
            Serveur distant : l'audio enregistré lui est envoyé.
            {#if s.url.trim().startsWith("http://")}Connexion non chiffrée (http).{/if}
          </p>
        {/if}

        <div class="mt-3 flex gap-1">
          <button class={btn} onclick={() => engines.test(s.id)}>Tester</button>
          <button class={btn} onclick={() => openEdit(s)}>Modifier</button>
          <button
            class={`${btn} ${pendingDelete === s.id ? "text-danger" : ""}`}
            onclick={() => remove(s.id)}
            onblur={() => (pendingDelete = null)}
          >
            {pendingDelete === s.id ? "Confirmer ?" : "Supprimer"}
          </button>
        </div>
      </div>
    {:else}
      <div class="rounded-xl border border-dashed border-border p-10 text-center text-sm text-muted">
        Aucun serveur configuré.
      </div>
    {/each}
  </div>
</div>