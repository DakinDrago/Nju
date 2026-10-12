import { invoke } from "@tauri-apps/api/core";

export type Settings = {
  /** Copie auto de la transcription dans le presse-papiers */
  copyAuto: boolean;
  /** Taille max d'un fichier déposé (Mo) */
  maxFileMB: number;
  /** Durée min d'un enregistrement micro (s) */
  minRecordSec: number;
  /** Export MMS : uniquement les transcriptions corrigées */
  correctedOnly: boolean;
  /** Langue ISO 639-3 pour l'export MMS */
  language: string;
  /** Locuteur pour l'export MMS */
  speaker: string;
  /** Découpage des longs audios : durée max par morceau (s) */
  chunkSecs: number;
  /** Timeout du test de connexion serveur (s) */
  serverTestTimeout: number;
  /** Timeout de la transcription serveur (s) */
  serverTranscribeTimeout: number;
  /** Nombre d'entrées chargées dans l'historique */
  historyLimit: number;
  /** Fermer = masquer en fond (tray) au lieu de quitter */
  runInBackground: boolean;
  /** Raccourci global actif ou non */
  hotkeyEnabled: boolean;
  /** Raccourci global (ex. "Ctrl+Shift+Space") */
  hotkey: string;
};

const KEY = "nju-settings";

const defaults: Settings = {
  copyAuto: true,
  maxFileMB: 200,
  minRecordSec: 0.3,
  correctedOnly: true,
  language: "fmp",
  speaker: "speaker_001",
  chunkSecs: 25,
  serverTestTimeout: 5,
  serverTranscribeTimeout: 120,
  historyLimit: 100,
  runInBackground: true,
  hotkeyEnabled: true,
  hotkey: "Ctrl+Shift+Space",
};

function sanitizeNumber(v: unknown, fallback: number, min: number, max: number): number {
  const n = typeof v === "number" ? v : Number(v);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(max, Math.max(min, n));
}

function sanitize(saved: Partial<Settings>): Settings {
  return {
    copyAuto: typeof saved.copyAuto === "boolean" ? saved.copyAuto : defaults.copyAuto,
    maxFileMB: sanitizeNumber(saved.maxFileMB, defaults.maxFileMB, 10, 2000),
    minRecordSec: sanitizeNumber(saved.minRecordSec, defaults.minRecordSec, 0.1, 5),
    correctedOnly: typeof saved.correctedOnly === "boolean" ? saved.correctedOnly : defaults.correctedOnly,
    language:
      typeof saved.language === "string" && saved.language.trim()
        ? saved.language.trim().slice(0, 16)
        : defaults.language,
    speaker:
      typeof saved.speaker === "string" && saved.speaker.trim()
        ? saved.speaker.trim().slice(0, 64)
        : defaults.speaker,
    chunkSecs: sanitizeNumber(saved.chunkSecs, defaults.chunkSecs, 5, 120),
    serverTestTimeout: Math.round(sanitizeNumber(saved.serverTestTimeout, defaults.serverTestTimeout, 1, 120)),
    serverTranscribeTimeout: Math.round(
      sanitizeNumber(saved.serverTranscribeTimeout, defaults.serverTranscribeTimeout, 10, 1800),
    ),
    historyLimit: Math.round(sanitizeNumber(saved.historyLimit, defaults.historyLimit, 10, 1000)),
    runInBackground: typeof saved.runInBackground === "boolean" ? saved.runInBackground : defaults.runInBackground,
    hotkeyEnabled: typeof saved.hotkeyEnabled === "boolean" ? saved.hotkeyEnabled : defaults.hotkeyEnabled,
    hotkey:
      typeof saved.hotkey === "string" && saved.hotkey.trim()
        ? saved.hotkey.trim().slice(0, 64)
        : defaults.hotkey,
  };
}

class SettingsStore {
  data = $state<Settings>({ ...defaults });

  /** Charge depuis localStorage puis applique les réglages côté natif (hotkey, tray). */
  async init() {
    try {
      const raw = localStorage.getItem(KEY);
      if (raw) this.data = sanitize({ ...defaults, ...JSON.parse(raw) });
    } catch {
      this.data = { ...defaults };
    }
    await this.applyNative();
  }

  /** Applique les réglages qui vivent côté natif (raccourci, comportement fermeture). */
  async applyNative() {
    try {
      await invoke("set_hotkey", { enabled: this.data.hotkeyEnabled, shortcut: this.data.hotkey });
    } catch (e) {
      console.error("Raccourci global non appliqué:", e);
    }
    try {
      await invoke("set_run_in_background", { enabled: this.data.runInBackground });
    } catch (e) {
      console.error("Comportement fermeture non appliqué:", e);
    }
  }

  update(patch: Partial<Settings>) {
    this.data = sanitize({ ...this.data, ...patch });
    try {
      localStorage.setItem(KEY, JSON.stringify(this.data));
    } catch {}
  }

  reset() {
    this.data = { ...defaults };
    try {
      localStorage.setItem(KEY, JSON.stringify(this.data));
    } catch {}
    void this.applyNative();
  }
}

export const settings = new SettingsStore();
export const SETTINGS_DEFAULTS = defaults;
