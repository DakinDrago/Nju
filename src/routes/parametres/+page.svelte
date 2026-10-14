<script lang="ts">
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Switch } from "$lib/components/ui/switch";
  import { Separator } from "$lib/components/ui/separator";
  import { Sun, Moon, Monitor, Keyboard, Mic, Database, SlidersHorizontal, RotateCcw, Save, RefreshCw, Download } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { toast } from "svelte-sonner";
  import { theme, type Theme } from "$lib/theme.svelte";
  import { settings } from "$lib/settings.svelte";
  import { updater } from "$lib/updater.svelte";
  import pkg from "../../../package.json";

  let appVersion = $state(pkg.version);

  onMount(async () => {
    try {
      appVersion = await getVersion();
    } catch {
      appVersion = pkg.version;
    }
    // Vérification silencieuse : ne toast que si une MAJ est dispo.
    try {
      await updater.checkForUpdates(true);
    } catch {
      // Ignoré (hors Tauri / hors ligne)
    }
  });

  async function checkUpdatesManual() {
    const found = await updater.checkForUpdates(false);
    if (found) toast.success(`Mise à jour ${updater.availableVersion} disponible`);
    else if (updater.status === "up-to-date") toast.success("Déjà à jour");
    else if (updater.status === "error") toast.error("Échec de la vérification : " + (updater.error ?? ""));
  }

  async function installUpdate() {
    await updater.downloadAndInstall();
    if (updater.status === "error") toast.error("Échec de l'installation : " + (updater.error ?? ""));
  }

  const options: { value: Theme; label: string; icon: typeof Sun }[] = [
    { value: "light", label: "Clair", icon: Sun },
    { value: "dark", label: "Sombre", icon: Moon },
    { value: "system", label: "Système", icon: Monitor },
  ];

  let savingHotkey = $state(false);

  async function applyHotkey() {
    savingHotkey = true;
    try {
      await invoke("set_hotkey", { enabled: settings.data.hotkeyEnabled, shortcut: settings.data.hotkey });
      toast.success("Raccourci appliqué");
    } catch (e) {
      toast.error("Raccourci refusé : " + String(e));
    } finally {
      savingHotkey = false;
    }
  }

  async function toggleHotkey(enabled: boolean) {
    settings.update({ hotkeyEnabled: enabled });
    await applyHotkey();
  }

  async function toggleBackground(enabled: boolean) {
    settings.update({ runInBackground: enabled });
    try {
      await invoke("set_run_in_background", { enabled });
    } catch (e) {
      toast.error("Erreur : " + String(e));
    }
  }

  function resetAll() {
    settings.reset();
    toast.success("Réglages réinitialisés");
  }
</script>

<div class="mx-auto max-w-2xl px-8 py-14 space-y-8">
  <div class="flex items-center justify-between">
    <h1 class="text-2xl font-semibold tracking-tight">Paramètres</h1>
    <Button variant="ghost" size="sm" onclick={resetAll}>
      <RotateCcw class="mr-2 h-3 w-3" />
      Réinitialiser
    </Button>
  </div>

  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-2"><Keyboard class="h-4 w-4" /> Raccourci global</CardTitle>
      <CardDescription class="text-xs">Démarrer / arrêter l'enregistrement depuis n'importe quelle application</CardDescription>
    </CardHeader>
    <CardContent class="space-y-4">
      <div class="flex items-center justify-between gap-4">
        <Label for="hotkey-enabled" class="text-sm">Raccourci actif</Label>
        <Switch
          id="hotkey-enabled"
          checked={settings.data.hotkeyEnabled}
          onCheckedChange={toggleHotkey}
        />
      </div>
      <div class="space-y-1.5">
        <Label for="hotkey" class="text-xs text-muted-foreground">Combinaison (ex. Ctrl+Shift+Space)</Label>
        <div class="flex gap-2">
          <Input
            id="hotkey"
            value={settings.data.hotkey}
            oninput={(e) => settings.update({ hotkey: (e.target as HTMLInputElement).value })}
            placeholder="Ctrl+Shift+Space"
            class="font-mono"
          />
          <Button size="sm" onclick={applyHotkey} disabled={savingHotkey} class="shrink-0">
            <Save class="mr-1 h-3 w-3" />
            Appliquer
          </Button>
        </div>
      </div>
      <p class="text-xs text-muted-foreground">La transcription est copiée automatiquement : colle avec Ctrl+V dans l'appli active.</p>
    </CardContent>
  </Card>

  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-2"><Mic class="h-4 w-4" /> Transcription</CardTitle>
      <CardDescription class="text-xs">Enregistrement et moteurs</CardDescription>
    </CardHeader>
    <CardContent class="space-y-4">
      <div class="grid grid-cols-2 gap-4">
        <div class="space-y-1.5">
          <Label for="min-record" class="text-xs text-muted-foreground">Durée min d'enregistrement (s)</Label>
          <Input
            id="min-record"
            type="number"
            min="0.1"
            max="5"
            step="0.1"
            value={settings.data.minRecordSec}
            oninput={(e) => settings.update({ minRecordSec: Number((e.target as HTMLInputElement).value) })}
          />
        </div>
        <div class="space-y-1.5">
          <Label for="max-file" class="text-xs text-muted-foreground">Taille max fichier déposé (Mo)</Label>
          <Input
            id="max-file"
            type="number"
            min="10"
            max="2000"
            step="10"
            value={settings.data.maxFileMB}
            oninput={(e) => settings.update({ maxFileMB: Number((e.target as HTMLInputElement).value) })}
          />
        </div>
      </div>
      <div class="space-y-1.5">
        <Label for="chunk-secs" class="text-xs text-muted-foreground">Découpage des longs audios : durée max par morceau (s)</Label>
        <Input
          id="chunk-secs"
          type="number"
          min="5"
          max="120"
          step="1"
          value={settings.data.chunkSecs}
          oninput={(e) => settings.update({ chunkSecs: Number((e.target as HTMLInputElement).value) })}
        />
      </div>
      <div class="grid grid-cols-2 gap-4">
        <div class="space-y-1.5">
          <Label for="timeout-test" class="text-xs text-muted-foreground">Timeout test serveur (s)</Label>
          <Input
            id="timeout-test"
            type="number"
            min="1"
            max="120"
            step="1"
            value={settings.data.serverTestTimeout}
            oninput={(e) => settings.update({ serverTestTimeout: Number((e.target as HTMLInputElement).value) })}
          />
        </div>
        <div class="space-y-1.5">
          <Label for="timeout-transcribe" class="text-xs text-muted-foreground">Timeout transcription serveur (s)</Label>
          <Input
            id="timeout-transcribe"
            type="number"
            min="10"
            max="1800"
            step="10"
            value={settings.data.serverTranscribeTimeout}
            oninput={(e) => settings.update({ serverTranscribeTimeout: Number((e.target as HTMLInputElement).value) })}
          />
        </div>
      </div>
    </CardContent>
  </Card>

  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-2"><Database class="h-4 w-4" /> Données & export MMS</CardTitle>
      <CardDescription class="text-xs">Historique et dataset d'entraînement</CardDescription>
    </CardHeader>
    <CardContent class="space-y-4">
      <div class="grid grid-cols-2 gap-4">
        <div class="space-y-1.5">
          <Label for="language" class="text-xs text-muted-foreground">Langue (ISO 639-3)</Label>
          <Input
            id="language"
            value={settings.data.language}
            oninput={(e) => settings.update({ language: (e.target as HTMLInputElement).value })}
            placeholder="fmp"
            class="font-mono"
          />
        </div>
        <div class="space-y-1.5">
          <Label for="speaker" class="text-xs text-muted-foreground">Locuteur</Label>
          <Input
            id="speaker"
            value={settings.data.speaker}
            oninput={(e) => settings.update({ speaker: (e.target as HTMLInputElement).value })}
            placeholder="speaker_001"
            class="font-mono"
          />
        </div>
      </div>
      <div class="flex items-center justify-between gap-4">
        <Label for="corrected-only" class="text-sm">Exporter uniquement les corrigées</Label>
        <Switch
          id="corrected-only"
          checked={settings.data.correctedOnly}
          onCheckedChange={(v) => settings.update({ correctedOnly: v })}
        />
      </div>
      <div class="space-y-1.5">
        <Label for="history-limit" class="text-xs text-muted-foreground">Entrées chargées dans l'historique</Label>
        <Input
          id="history-limit"
          type="number"
          min="10"
          max="1000"
          step="10"
          value={settings.data.historyLimit}
          oninput={(e) => settings.update({ historyLimit: Number((e.target as HTMLInputElement).value) })}
        />
      </div>
    </CardContent>
  </Card>

  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-2"><SlidersHorizontal class="h-4 w-4" /> Interface</CardTitle>
      <CardDescription class="text-xs">Apparence et comportement</CardDescription>
    </CardHeader>
    <CardContent class="space-y-4">
      <div>
        <Label class="text-xs text-muted-foreground">Thème</Label>
        <div class="mt-2 inline-flex gap-1 rounded-lg border border-border bg-card p-1" role="group" aria-label="Sélection du thème">
          {#each options as option}
            <Button
              variant={theme.mode === option.value ? "default" : "ghost"}
              size="sm"
              class="flex items-center gap-2 px-3 py-1.5"
              onclick={() => theme.set(option.value)}
            >
              <option.icon class="h-4 w-4" />
              {option.label}
            </Button>
          {/each}
        </div>
      </div>
      <div class="flex items-center justify-between gap-4">
        <div>
          <Label for="copy-auto" class="text-sm">Copie automatique</Label>
          <p class="text-xs text-muted-foreground">Copier chaque transcription dans le presse-papiers</p>
        </div>
        <Switch
          id="copy-auto"
          checked={settings.data.copyAuto}
          onCheckedChange={(v) => settings.update({ copyAuto: v })}
        />
      </div>
      <div class="flex items-center justify-between gap-4">
        <div>
          <Label for="background" class="text-sm">Rester en fond</Label>
          <p class="text-xs text-muted-foreground">Fermer la fenêtre la masque (icône de notification) au lieu de quitter</p>
        </div>
        <Switch
          id="background"
          checked={settings.data.runInBackground}
          onCheckedChange={toggleBackground}
        />
      </div>
    </CardContent>
  </Card>

  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-2"><RefreshCw class="h-4 w-4" /> Mises à jour</CardTitle>
      <CardDescription class="text-xs">Version {appVersion} — canal stable GitHub Releases</CardDescription>
    </CardHeader>
    <CardContent class="space-y-3 text-sm">
      {#if updater.status === "checking"}
        <p class="text-xs text-muted-foreground">Vérification en cours…</p>
      {:else if updater.status === "available" || updater.status === "downloading" || updater.status === "ready"}
        <div class="flex items-center justify-between gap-4">
          <p class="text-sm">Version {updater.availableVersion} disponible</p>
          <Button
            size="sm"
            onclick={installUpdate}
            disabled={updater.status === "downloading" || updater.status === "ready"}
          >
            <Download class="mr-1 h-3 w-3" />
            {updater.status === "downloading" ? `Installation… ${updater.progress}%` : updater.status === "ready" ? "Redémarrage…" : "Installer et relancer"}
          </Button>
        </div>
        {#if updater.status === "downloading"}
          <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
            <div class="h-full rounded-full bg-primary transition-all" style="width: {updater.progress}%"></div>
          </div>
        {/if}
        {#if updater.notes}
          <p class="text-xs text-muted-foreground whitespace-pre-line">{updater.notes}</p>
        {/if}
      {:else if updater.status === "up-to-date"}
        <div class="flex items-center justify-between gap-4">
          <p class="text-xs text-muted-foreground">Tu es à jour.</p>
          <Button variant="ghost" size="sm" onclick={checkUpdatesManual}>Revérifier</Button>
        </div>
      {:else if updater.status === "error"}
        <div class="flex items-center justify-between gap-4">
          <p class="text-xs text-destructive">Échec : {updater.error}</p>
          <Button variant="ghost" size="sm" onclick={checkUpdatesManual}>Réessayer</Button>
        </div>
      {:else}
        <div class="flex items-center justify-between gap-4">
          <p class="text-xs text-muted-foreground">Vérifie si une nouvelle version est publiée.</p>
          <Button variant="outline" size="sm" onclick={checkUpdatesManual}>
            <RefreshCw class="mr-1 h-3 w-3" />
            Vérifier les mises à jour
          </Button>
        </div>
      {/if}
    </CardContent>
  </Card>

  <Card>
    <CardHeader>
      <CardTitle>À propos</CardTitle>
      <CardDescription class="text-xs">Informations sur l'application</CardDescription>
    </CardHeader>
    <CardContent class="space-y-3 text-sm text-muted-foreground">
      <div class="flex justify-between">
        <span>Version</span>
        <span class="font-mono">{appVersion}</span>
      </div>
      <div class="flex justify-between">
        <span>Moteur</span>
        <span>Tauri + SvelteKit</span>
      </div>
      <Separator />
      <p class="text-xs">Njū' - Speech-to-text pour le fe'efe'e</p>
    </CardContent>
  </Card>
</div>
