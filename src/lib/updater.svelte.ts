import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "up-to-date"
  | "error";

class UpdaterState {
  status = $state<UpdateStatus>("idle");
  availableVersion = $state<string | null>(null);
  notes = $state<string | null>(null);
  progress = $state(0);
  error = $state<string | null>(null);
  private update: Awaited<ReturnType<typeof check>> = null;

  /** Vérifie la présence d'une MAJ. silent=true : pas d'état "up-to-date"/"error" persistant. */
  async checkForUpdates(silent = false): Promise<boolean> {
    this.status = "checking";
    this.error = null;
    this.progress = 0;
    try {
      const update = await check();
      this.update = update;
      if (update) {
        this.status = "available";
        this.availableVersion = update.version;
        this.notes = update.body ?? null;
        return true;
      }
      this.status = silent ? "idle" : "up-to-date";
      this.availableVersion = null;
      return false;
    } catch (e) {
      // Hors Tauri (navigateur dev) ou sans réseau : on reste silencieux en mode silent.
      this.update = null;
      if (silent) {
        this.status = "idle";
        return false;
      }
      this.status = "error";
      this.error = String(e);
      return false;
    }
  }

  /** Télécharge + installe la MAJ en attente, puis relance l'app. */
  async downloadAndInstall(): Promise<void> {
    if (!this.update) {
      const found = await this.checkForUpdates();
      if (!found || !this.update) return;
    }
    this.status = "downloading";
    this.progress = 0;
    this.error = null;
    try {
      let downloaded = 0;
      let contentLength = 0;
      await this.update!.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength ?? 0;
            this.progress = 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            this.progress = contentLength > 0 ? Math.round((downloaded / contentLength) * 100) : 0;
            break;
          case "Finished":
            this.progress = 100;
            break;
        }
      });
      this.status = "ready";
      await relaunch();
    } catch (e) {
      this.status = "error";
      this.error = String(e);
    }
  }

  reset() {
    this.status = "idle";
    this.availableVersion = null;
    this.notes = null;
    this.progress = 0;
    this.error = null;
    this.update = null;
  }
}

export const updater = new UpdaterState();
