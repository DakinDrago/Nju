<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { engines, dotClass, localKey } from "$lib/engines.svelte";
  import { transcribe, cancelTranscription } from "$lib/transcribe";
  import { Button } from "$lib/components/ui/button";
  import { Card, CardHeader, CardTitle, CardContent } from "$lib/components/ui/card";
  import {
    DropdownMenu,
    DropdownMenuTrigger,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
  } from "$lib/components/ui/dropdown-menu";
  import { microphone } from "$lib/stores";
  import { settings } from "$lib/settings.svelte";
  import { Mic, MicOff, Copy, Check, AlertCircle, FileUp, Loader2 } from "lucide-svelte";
  import { toast } from "svelte-sonner";
  import { listen } from "@tauri-apps/api/event";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";

  const engineState = $derived(
    engines.active ? (engines.status[engines.active.id]?.state ?? "unknown") : "unknown",
  );

  // Tous les moteurs (modèles locaux + serveurs) pour le sélecteur rapide
  const allEngines = $derived([
    ...engines.locals.map((m) => ({ id: localKey(m.id), name: m.name })),
    ...engines.servers.map((s) => ({ id: s.id, name: s.name })),
  ]);

  let status = $state("Prêt");
  let result = $state("");
  let recording = $state(false);
  let transcribing = $state(false);
  let progress = $state<{ done: number; total: number } | null>(null);
  let copied = $state(false);
  let processingFile = $state(false);
  let dragOver = $state(false);
  let elapsedSec = $state(0);

  let fileInput: HTMLInputElement | null = null;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;

  let stream: MediaStream | null = null;
  let mediaRecorder: MediaRecorder | null = null;
  let recordedChunks: Blob[] = [];
  let recordStartTime = 0;

  // Check if model is loaded
  const modelLoaded = $derived(engines.active && engines.status[engines.active.id]?.state === "online");
  const modelStatus = $derived(engines.active ? engines.status[engines.active.id]?.state : "unknown");

  async function acquireStream(deviceId?: string | null) {
    try {
      stream?.getTracks().forEach((t) => t.stop());
    } catch { /* ignore */ }
    const constraints: MediaStreamConstraints = deviceId
      ? { audio: { deviceId: { exact: deviceId } } }
      : { audio: true };
    try {
      stream = await navigator.mediaDevices.getUserMedia(constraints);
    } catch (e) {
      console.warn("Micro sélectionné indisponible, utilisation du micro par défaut:", e);
      try {
        stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      } catch (err) {
        status = "Impossible d'accéder au microphone";
        console.error(err);
      }
    }
  }

  onMount(() => {
    let cancelled = false;
    const unsub = microphone.subscribe((id) => {
      if (cancelled || recording) return;
      void acquireStream(id || undefined);
    });

    // Raccourci global (Ctrl+Maj+Espace) : bascule l'enregistrement depuis n'importe quelle appli.
    let unlistenToggle: (() => void) | null = null;
    listen("toggle-recording", () => {
      if (recording) void stop();
      else void start();
    })
      .then((f) => {
        unlistenToggle = f;
      })
      .catch((e) => console.error("Écoute toggle-recording impossible:", e));

    return () => {
      cancelled = true;
      unsub();
      unlistenToggle?.();
      stream?.getTracks().forEach((t) => t.stop());
    };
  });

  async function start() {
    if (!stream || recording) return;
    
    if (!modelLoaded) {
      status = "Aucun modèle chargé. Allez dans Modèles pour en sélectionner un.";
      toast.error("Aucun modèle chargé. Allez dans Modèles pour en sélectionner un.");
      return;
    }

    recordedChunks = [];
    
    const mimeType = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
      ? "audio/webm;codecs=opus"
      : MediaRecorder.isTypeSupported("audio/webm")
        ? "audio/webm"
        : "";
    
    try {
      mediaRecorder = new MediaRecorder(stream, mimeType ? { mimeType } : undefined);
    } catch (e) {
      console.warn("MediaRecorder fallback:", e);
      mediaRecorder = new MediaRecorder(stream);
    }

    mediaRecorder.ondataavailable = (e) => {
      if (e.data.size > 0) recordedChunks.push(e.data);
    };

    mediaRecorder.onstop = async () => {
      const blob = new Blob(recordedChunks, { type: mediaRecorder?.mimeType || "audio/webm" });
      recordedChunks = [];
      
      // Durée mesurée au chrono (fiable, indépendante du codec ; seuil réglable dans Paramètres)
      const elapsedSec = (Date.now() - recordStartTime) / 1000;
      if (elapsedSec < settings.data.minRecordSec) {
        status = "Trop court, maintiens plus longtemps";
        return;
      }

      status = "Transcription en cours...";
      transcribing = true;
      progress = null;
      try {
        result = await transcribe(blob, {
          chunkSecs: settings.data.chunkSecs,
          timeoutSecs: settings.data.serverTranscribeTimeout,
          onProgress: (done, total) => {
            progress = { done, total };
            status = total > 1 ? `Transcription ${done}/${total}…` : "Transcription en cours...";
          },
        });
        status = "Terminé";
        copied = false;
        // Copie auto (réglable dans Paramètres) pour coller ensuite dans l'appli active (Ctrl+V).
        if (settings.data.copyAuto) {
          try {
            await writeText(result);
            toast.success("Transcription copiée — colle avec Ctrl+V");
          } catch (e) {
            console.warn("Copie presse-papiers impossible:", e);
          }
        }
      } catch (err) {
        result = "Erreur : " + String(err);
        status = "Erreur";
        console.error(err);
      } finally {
        transcribing = false;
        progress = null;
      }
    };

    mediaRecorder.start(100);
    recordStartTime = Date.now();
    recording = true;
    status = "Enregistrement...";
  }

  async function stop() {
    if (!recording || !mediaRecorder) return;
    recording = false;
    mediaRecorder.stop();
  }

  async function cancel() {
    await cancelTranscription();
  }

  async function copyToClipboard() {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(result);
      copied = true;
      toast.success("Copié dans le presse-papiers");
      setTimeout(() => copied = false, 2000);
    } catch (e) {
      toast.error("Impossible de copier");
    }
  }

  function formatElapsed(sec: number): string {
    const m = Math.floor(sec / 60);
    const s = Math.floor(sec % 60);
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  function startElapsedTimer() {
    stopElapsedTimer();
    elapsedSec = 0;
    elapsedTimer = setInterval(() => {
      elapsedSec += 1;
    }, 1000);
  }

  function stopElapsedTimer() {
    if (elapsedTimer) {
      clearInterval(elapsedTimer);
      elapsedTimer = null;
    }
  }

  // Transcrit un fichier audio déposé ou choisi (réutilise le pipeline de transcription)
  async function transcribeFile(file: File) {
    if (processingFile || recording) return;

    if (!modelLoaded) {
      status = "Aucun modèle chargé. Allez dans Modèles pour en sélectionner un.";
      toast.error("Aucun modèle chargé. Allez dans Modèles pour en sélectionner un.");
      return;
    }

    if (!file.type.startsWith("audio/")) {
      status = "Fichier non supporté : dépose un fichier audio.";
      toast.error("Fichier non supporté : dépose un fichier audio.");
      return;
    }

    if (file.size > settings.data.maxFileMB * 1024 * 1024) {
      status = `Fichier trop volumineux (max ${settings.data.maxFileMB} Mo, réglable dans Paramètres).`;
      toast.error(`Fichier trop volumineux (max ${settings.data.maxFileMB} Mo).`);
      return;
    }

    processingFile = true;
    startElapsedTimer();
    status = `Transcription de ${file.name} en cours...`;
    transcribing = true;
    progress = null;
    try {
      result = await transcribe(file, {
        chunkSecs: settings.data.chunkSecs,
        timeoutSecs: settings.data.serverTranscribeTimeout,
        onProgress: (done, total) => {
          progress = { done, total };
          status = total > 1 ? `Transcription de ${file.name} ${done}/${total}…` : `Transcription de ${file.name} en cours...`;
        },
      });
      status = "Terminé";
      copied = false;
      // Copie auto (réglable dans Paramètres) pour coller ensuite dans l'appli active (Ctrl+V).
      if (settings.data.copyAuto) {
        try {
          await writeText(result);
          toast.success("Transcription copiée — colle avec Ctrl+V");
        } catch (e) {
          console.warn("Copie presse-papiers impossible:", e);
        }
      }
    } catch (err) {
      result = "Erreur : " + String(err);
      status = "Erreur";
      console.error(err);
    } finally {
      processingFile = false;
      transcribing = false;
      progress = null;
      stopElapsedTimer();
    }
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) void transcribeFile(file);
  }

  function handleFileInput(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) void transcribeFile(file);
    input.value = "";
  }
</script>

<div class="mx-auto flex w-full max-w-2xl flex-col items-center px-4 py-8 text-center md:px-8 md:py-12">
  <h1 class="text-2xl font-semibold tracking-tight">Nouvelle transcription</h1>
  <p class="mt-2 mb-5 text-sm text-muted-foreground">Maintenir le bouton pour parler</p>

  <!-- Model status indicator + sélecteur rapide -->
  <div class="mb-6 flex items-center justify-center gap-2">
    <DropdownMenu>
      <DropdownMenuTrigger
        class="inline-flex items-center gap-2 rounded-full border border-border bg-card px-3 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
        aria-label="Choisir le moteur de transcription"
      >
        <span class={`size-2 rounded-full ${dotClass[engineState]}`}></span>
        {engines.active?.name ?? "Aucun moteur"}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="center" class="w-64">
        <DropdownMenuLabel class="text-xs">Choisir un moteur</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {#each allEngines as e (e.id)}
          {@const st = engines.status[e.id]?.state ?? "unknown"}
          <DropdownMenuItem onSelect={() => engines.setActive(e.id)} class="gap-2">
            <span class={`size-2 shrink-0 rounded-full ${dotClass[st]}`}></span>
            <span class="truncate">{e.name}</span>
            {#if engines.activeId === e.id}
              <Check class="ml-auto h-3 w-3 shrink-0" />
            {/if}
          </DropdownMenuItem>
        {:else}
          <DropdownMenuItem disabled>Aucun moteur configuré</DropdownMenuItem>
        {/each}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => goto("/modeles")} class="gap-2 text-muted-foreground">
          Gérer les modèles…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
    
    {#if !modelLoaded}
      <div class="inline-flex items-center gap-1.5 rounded-lg bg-destructive/10 border border-destructive/20 px-3 py-1.5 text-sm text-destructive">
        <AlertCircle class="h-3 w-3 flex-shrink-0" />
        <span class="text-xs">Modèle non chargé</span>
      </div>
    {/if}
  </div>

  <Button
    class="grid size-28 touch-none place-items-center rounded-full text-base font-medium shadow-lg transition select-none md:size-36"
    variant={recording ? "destructive" : "default"}
    size="icon"
    onpointerdown={(e) => {
      e.currentTarget.setPointerCapture(e.pointerId);
      start();
    }}
    onpointerup={stop}
    onpointercancel={stop}
    oncontextmenu={(e) => e.preventDefault()}
    aria-label={recording ? "Arrêter l'enregistrement" : "Démarrer l'enregistrement"}
    disabled={!modelLoaded}
  >
    <span class="flex flex-col items-center gap-1">
      {#if recording}
        <Mic class="size-6" />
        <span class="text-xs">Relâcher</span>
      {:else}
        <MicOff class="size-6" />
        <span class="text-xs">Maintenir</span>
      {/if}
    </span>
  </Button>

  {#if !recording}
    <p class="mt-6 min-h-6 text-sm text-muted-foreground">{status}</p>
    {#if transcribing}
      <Button variant="outline" size="sm" onclick={cancel} class="mt-2">
        Annuler
      </Button>
    {/if}
  {/if}

  <section class="mt-10 w-full text-left">
    <Card>
      <CardHeader class="flex flex-row items-center justify-between">
        <CardTitle class="text-xs font-medium tracking-wide text-muted-foreground uppercase">Transcription</CardTitle>
        {#if result}
          <Button
            variant="ghost"
            size="icon"
            onclick={copyToClipboard}
            class={copied ? "text-green-500" : ""}
            aria-label={copied ? "Copié !" : "Copier la transcription"}
          >
            {#if copied}
              <Check class="h-4 w-4 text-green-500" />
            {:else}
              <Copy class="h-4 w-4" />
            {/if}
          </Button>
        {/if}
      </CardHeader>
      <CardContent class="min-h-28 text-base leading-relaxed">
        {#if result}
          <div class="whitespace-pre-wrap">{result}</div>
        {:else}
          <p class="text-muted-foreground">La transcription apparaîtra ici...</p>
        {/if}
      </CardContent>
    </Card>
  </section>

  <section class="mt-6 w-full text-left">
    <div
      role="button"
      tabindex="0"
      aria-label="Déposer un fichier audio à transcrire"
      class={`flex flex-col items-center justify-center gap-2 rounded-xl border border-dashed p-8 text-center transition-colors ${
        dragOver
          ? "border-accent bg-accent/10"
          : "border-border bg-surface hover:border-accent/50"
      } ${processingFile ? "pointer-events-none opacity-70" : "cursor-pointer"}`}
      ondragover={(e) => {
        e.preventDefault();
        dragOver = true;
      }}
      ondragleave={() => (dragOver = false)}
      ondrop={handleDrop}
      onclick={() => fileInput?.click()}
      onkeydown={(e) => {
        if (e.key === "Enter" || e.key === " ") fileInput?.click();
      }}
    >
      {#if processingFile}
        <Loader2 class="h-8 w-8 animate-spin text-accent" />
        <p class="text-sm font-medium">
          {#if progress && progress.total > 1}
            Transcription en cours... {progress.done}/{progress.total} morceaux · {formatElapsed(elapsedSec)}
          {:else}
            Transcription en cours... {formatElapsed(elapsedSec)}
          {/if}
        </p>
        <p class="text-xs text-muted-foreground">Les longs audios sont découpés automatiquement, patiente.</p>
        <Button variant="outline" size="sm" onclick={cancel}>
          Annuler
        </Button>
      {:else}
        <FileUp class="h-8 w-8 text-muted-foreground" />
        <p class="text-sm font-medium">Glisse un fichier audio ici</p>
        <p class="text-xs text-muted-foreground">ou clique pour choisir (mp3, wav, ogg, webm... · max {settings.data.maxFileMB} Mo)</p>
      {/if}
      <input
        type="file"
        accept="audio/*"
        class="hidden"
        bind:this={fileInput}
        onchange={handleFileInput}
      />
    </div>
  </section>
</div>
