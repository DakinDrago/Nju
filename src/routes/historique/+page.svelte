<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { Textarea } from "$lib/components/ui/textarea";
  import { AlertDialog, AlertDialogTrigger, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from "$lib/components/ui/alert-dialog";
  import { Toaster } from "$lib/components/ui/sonner";
  import { toast } from "svelte-sonner";
  import { History, Play, Pause, Edit2, Save, Trash2, Download, FileText, Music, Loader2, X, ChevronLeft, CheckCircle, Clock, Mic } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { settings } from "$lib/settings.svelte";

  type TranscriptionStatus = "original" | "pending" | "corrected";

  interface Transcription {
    id: string;
    timestamp: string;
    engine: string;
    audio_file: string;
    original_text: string;
    corrected_text: string;
    duration_sec: number;
    sample_rate: number;
    status: TranscriptionStatus;
  }

  let history = $state<Transcription[]>([]);
  let loading = $state(false);
  let selectedId = $state<string | null>(null);
  let detail = $state<Transcription | null>(null);
  let editedText = $state("");
  let saving = $state(false);
  let exporting = $state(false);
  let audioPlaying = $state(false);
  let audioElement: HTMLAudioElement | null = null;

  const statusConfig: Record<TranscriptionStatus, { label: string; color: string; icon: any }> = {
    corrected: { label: "Corrigé", color: "bg-emerald-500", icon: CheckCircle },
    pending: { label: "À corriger", color: "bg-amber-500", icon: Clock },
    original: { label: "Original", color: "bg-muted/50", icon: FileText },
  };

  async function loadHistory() {
    loading = true;
    try {
      history = await invoke<Transcription[]>("list_history", { limit: settings.data.historyLimit });
    } catch (e) {
      toast.error("Erreur chargement historique: " + String(e));
    } finally {
      loading = false;
    }
  }

  async function selectItem(id: string) {
    if (selectedId === id) {
      selectedId = null;
      detail = null;
      return;
    }
    selectedId = id;
    try {
      const t = await invoke<Transcription | null>("get_transcription", { id });
      if (t) {
        detail = t;
        editedText = t.corrected_text || t.original_text;
      }
    } catch (e) {
      toast.error("Erreur: " + String(e));
    }
  }

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleString("fr-FR", { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" });
  }

  function formatDuration(sec: number): string {
    const m = Math.floor(sec / 60);
    const s = Math.floor(sec % 60);
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  function getAudioUrl(audioFile: string): string {
    // L'audio est servi depuis le dossier app_data/history/audio/
    // En dev, on passe par invoke pour lire le fichier
    return `asset://local/${audioFile}`;
  }

  function mimeForFile(audioFile: string): string {
    const ext = audioFile.split(".").pop()?.toLowerCase() ?? "";
    switch (ext) {
      case "wav": return "audio/wav";
      case "mp3": return "audio/mpeg";
      case "ogg":
      case "opus": return "audio/ogg";
      case "webm": return "audio/webm";
      case "m4a": return "audio/mp4";
      case "flac": return "audio/flac";
      default: return "audio/wav";
    }
  }

  async function playAudio(audioFile: string) {
    if (audioPlaying) {
      audioElement?.pause();
      audioPlaying = false;
      return;
    }
    try {
      const base64 = await invoke<string>("read_audio_file", { audioFile });
      if (!audioElement) {
        audioElement = new Audio();
        audioElement.onended = () => { audioPlaying = false; };
      }
      audioElement.src = `data:${mimeForFile(audioFile)};base64,${base64}`;
      await audioElement.play();
      audioPlaying = true;
    } catch (e) {
      toast.error("Erreur lecture audio: " + String(e));
    }
  }

  async function saveCorrection() {
    if (!detail || saving) return;
    saving = true;
    try {
      await invoke("update_transcription", { id: detail.id, correctedText: editedText });
      detail = { ...detail, corrected_text: editedText, status: editedText.trim() ? "corrected" : "pending" };
      const idx = history.findIndex(h => h.id === detail!.id);
      if (idx >= 0) history[idx] = detail;
      toast.success("Correction sauvegardée");
    } catch (e) {
      toast.error("Erreur: " + String(e));
    } finally {
      saving = false;
    }
  }

  async function deleteItem(id: string) {
    try {
      console.log("DEBUG deleteItem: calling delete_transcription with id:", id);
      await invoke("delete_transcription", { id });
      console.log("DEBUG deleteItem: invoke succeeded");
      history = history.filter(h => h.id !== id);
      if (selectedId === id) {
        selectedId = null;
        detail = null;
      }
      toast.success("Supprimé");
    } catch (e) {
      console.error("DEBUG deleteItem: error:", e);
      toast.error("Erreur: " + String(e));
    }
  }

  async function exportMMS() {
    const dir = await open({ directory: true, title: "Choisis le dossier de sortie pour l'export MMS" });
    if (!dir || typeof dir !== "string") return;

    exporting = true;
    try {
      const path = await invoke<string>("export_mms_tsv", {
        outputDir: dir,
        correctedOnly: settings.data.correctedOnly,
        language: settings.data.language,
        speaker: settings.data.speaker,
      });
      toast.success(`Exporté dans ${path}`);
    } catch (e) {
      toast.error("Erreur export: " + String(e));
    } finally {
      exporting = false;
    }
  }

  onMount(() => {
    loadHistory();

    // Cleanup audio on unmount
    return () => {
      if (audioElement) {
        audioElement.pause();
        audioElement.src = "";
      }
    };
  });
</script>

<Toaster />

<div class="mx-auto max-w-4xl px-4 py-8 md:px-8 md:py-12">
  <div class="flex items-center justify-between gap-4 mb-8">
    <div class="flex items-center gap-2">
      <History class="h-6 w-6 text-muted-foreground" />
      <h1 class="text-2xl font-semibold tracking-tight">Historique & Correction</h1>
    </div>
    <Button variant="outline" size="sm" onclick={exportMMS} disabled={exporting || history.length === 0}>
      <Download class="mr-2 h-3 w-3" />
      {exporting ? "Export en cours..." : "Exporter MMS (.tsv)"}
    </Button>
  </div>

  <p class="mb-6 text-sm text-muted-foreground">
    {history.length} transcription{history.length > 1 ? "s" : ""} — Clique pour corriger, exporte les corrigées pour MMS-1B-all
  </p>

  <div class="grid gap-6 lg:grid-cols-[280px_1fr]">
    <!-- Liste -->
    <Card class="lg:sticky lg:top-24 h-fit">
      <CardHeader class="pb-3">
        <CardTitle class="text-base">Transcriptions</CardTitle>
        <CardDescription class="text-xs">
          {#if loading}
            Chargement...
          {:else}
            {history.length} élément{history.length > 1 ? "s" : ""}
          {/if}
        </CardDescription>
      </CardHeader>
      <CardContent class="p-0">
        <ScrollArea class="h-[calc(100vh-300px)]">
          <div class="p-3 space-y-2">
            {#if loading}
              <div class="flex items-center justify-center py-8">
                <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            {:else if history.length === 0}
              <div class="text-center py-8 text-sm text-muted-foreground">
                <History class="h-10 w-10 mx-auto mb-2 opacity-30" />
                <p>Aucune transcription</p>
                <p class="mt-1">Enregistre ta première transcription</p>
              </div>
            {:else}
              {#each history as t (t.id)}
                <button
                  class={cn(
                    "w-full text-left p-3 rounded-lg border transition-all",
                    selectedId === t.id
                      ? "border-primary bg-primary/5"
                      : "border-border hover:border-accent hover:bg-muted/30"
                  )}
                  onclick={() => selectItem(t.id)}
                >
                  <div class="flex items-start justify-between gap-2">
                    <div class="min-w-0 flex-1">
                      <div class="flex items-center gap-2 flex-wrap">
                        <Badge variant="outline" class={cn("gap-1 px-2 py-0.5 text-xs", statusConfig[t.status].color)}>
                          {@const StatusIcon = statusConfig[t.status].icon}
                          <StatusIcon class="h-2.5 w-2.5" />
                          {statusConfig[t.status].label}
                        </Badge>
                        <span class="text-xs text-muted-foreground truncate">{t.engine}</span>
                      </div>
                      <p class="mt-1 text-sm font-medium truncate">{t.original_text || t.corrected_text}</p>
                      <div class="mt-1 flex items-center gap-3 text-xs text-muted-foreground">
                        <span class="flex items-center gap-1"><Clock class="h-2.5 w-2.5" />{formatDuration(t.duration_sec)}</span>
                        <span>{formatDate(t.timestamp)}</span>
                      </div>
                    </div>
                  </div>
                </button>
              {/each}
            {/if}
          </div>
        </ScrollArea>
      </CardContent>
    </Card>

    <!-- Détail / Édition -->
    <Card>
      {#if detail}
        <CardHeader>
          <div class="flex items-start justify-between">
            <div>
              <CardTitle class="text-base">Correction</CardTitle>
              <CardDescription class="text-xs">
                {detail.engine} · {formatDate(detail.timestamp)} · {formatDuration(detail.duration_sec)}
              </CardDescription>
            </div>
            <div class="flex items-center gap-2">
              <Button variant="ghost" size="icon" onclick={() => playAudio(detail.audio_file)} disabled={audioPlaying}>
                {#if audioPlaying}
                  <Pause class="h-4 w-4" />
                {:else}
                  <Play class="h-4 w-4" />
                {/if}
              </Button>
              <AlertDialog>
                <AlertDialogTrigger asChild>
                  <Button variant="ghost" size="icon" class="text-destructive hover:text-destructive">
                    <Trash2 class="h-4 w-4" />
                  </Button>
                </AlertDialogTrigger>
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle>Supprimer cette transcription ?</AlertDialogTitle>
                    <AlertDialogDescription>L'audio et le texte seront supprimés définitivement.</AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel>Annuler</AlertDialogCancel>
                    <Button
                      variant="destructive"
                      size="sm"
                      onclick={() => {
                        deleteItem(detail.id);
                      }}
                    >
                      Supprimer
                    </Button>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
          </div>
        </CardHeader>
        <CardContent class="space-y-4">
          <!-- Lecteur audio -->
          <div class="flex items-center gap-3 p-3 rounded-lg border border-border bg-muted/30">
            <Mic class="h-5 w-5 text-muted-foreground" />
            <div class="flex-1">
              <p class="text-xs text-muted-foreground">Fichier audio original</p>
              <p class="text-sm font-mono text-muted-foreground">{detail.audio_file}</p>
            </div>
            <Button variant="outline" size="sm" onclick={() => playAudio(detail.audio_file)} disabled={audioPlaying}>
              {#if audioPlaying}
                <Pause class="mr-1 h-3 w-3" />
                <span>Lecture...</span>
              {:else}
                <Play class="mr-1 h-3 w-3" />
                <span>Écouter</span>
              {/if}
            </Button>
          </div>

          <Separator />

          <!-- Texte original (lecture seule) -->
          <div class="space-y-1.5">
            <Label class="text-xs text-muted-foreground flex items-center gap-1">
              <FileText class="h-3 w-3" />
              Texte original (modèle)
            </Label>
            <div class="p-3 rounded-lg border border-border bg-muted/30 text-sm whitespace-pre-wrap min-h-[60px] font-mono">
              {detail.original_text || "—"}
            </div>
          </div>

          <Separator />

          <!-- Texte corrigé (éditable) -->
          <div class="space-y-1.5">
            <Label class="text-xs text-muted-foreground flex items-center gap-1">
              <Edit2 class="h-3 w-3" />
              Texte corrigé
            </Label>
            <Textarea
              bind:value={editedText}
              placeholder="Corrige la transcription ici..."
              class="min-h-[120px] font-mono text-sm"
              rows={6}
            />
            <div class="flex justify-end gap-2">
              <Button variant="ghost" size="sm" onclick={() => editedText = detail.original_text}>
                <X class="mr-1 h-3 w-3" />
                Réinitialiser
              </Button>
              <Button size="sm" onclick={saveCorrection} disabled={saving || editedText.trim() === detail.corrected_text.trim()}>
                <Save class="mr-1 h-3 w-3" />
                {saving ? "Sauvegarde..." : "Sauvegarder correction"}
              </Button>
            </div>
            {#if detail.status === "corrected"}
              <p class="text-xs text-emerald-600 dark:text-emerald-400 flex items-center gap-1">
                <CheckCircle class="h-3 w-3" /> Corrigé et prêt pour l'export MMS
              </p>
            {:else if detail.status === "pending"}
              <p class="text-xs text-amber-600 dark:text-amber-400">En attente de correction</p>
            {:else}
              <p class="text-xs text-muted-foreground">Non corrigé — ne sera pas exporté</p>
            {/if}
          </div>
        </CardContent>
      {:else}
        <CardContent class="flex h-96 items-center justify-center">
          <div class="text-center text-muted-foreground">
            <History class="h-16 w-16 mx-auto mb-4 opacity-30" />
            <p class="text-lg">Sélectionne une transcription</p>
            <p class="text-sm mt-1">Pour la corriger ou l'écouter</p>
          </div>
        </CardContent>
      {/if}
    </Card>
  </div>
</div>