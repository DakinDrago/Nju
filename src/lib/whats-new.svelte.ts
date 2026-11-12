import { getVersion } from "@tauri-apps/api/app";
import pkg from "../../package.json";
import {
  compareVersions,
  releaseForVersion,
  releasesSince,
  type ChangelogRelease,
} from "./changelog";

const LAST_SEEN_KEY = "nju-last-seen-version";

function readLastSeen(): string | null {
  try {
    const value = localStorage.getItem(LAST_SEEN_KEY);
    return value && /^\d+(\.\d+)*$/.test(value) ? value : null;
  } catch {
    return null;
  }
}

function writeLastSeen(version: string) {
  try {
    localStorage.setItem(LAST_SEEN_KEY, version);
  } catch {
    /* stockage indisponible : on ne bloque pas l'application */
  }
}

/**
 * Vrai si l'application a déjà tourné sur cette machine (réglages, moteurs,
 * thème…). Sert à distinguer une installation neuve d'une mise à jour depuis
 * une version antérieure à l'introduction du changelog.
 */
function hasPreviousData(): boolean {
  try {
    return Object.keys(localStorage).some((key) => key.startsWith("nju-"));
  } catch {
    return false;
  }
}

class WhatsNewStore {
  /** Fenêtre « Nouveautés » ouverte. */
  open = $state(false);
  /** Versions à afficher, de la plus récente à la plus ancienne. */
  releases = $state<ChangelogRelease[]>([]);
  currentVersion = $state(pkg.version);
  /** L'initialisation (lecture de version) est terminée. */
  ready = $state(false);

  private initialized = false;

  /**
   * Détermine s'il faut afficher les nouveautés. À appeler avant les autres
   * initialisations qui écrivent dans localStorage (`engines.init()`), car
   * l'absence de données `nju-*` distingue une installation neuve.
   */
  async init() {
    if (this.initialized) return;
    this.initialized = true;

    // Instantané synchrone, avant tout `await` et avant les autres `init()`.
    const lastSeen = readLastSeen();
    const hadPreviousData = hasPreviousData();

    let version = pkg.version;
    try {
      version = await getVersion();
    } catch {
      // Hors Tauri (dev navigateur) : la version du package fait foi.
    }
    this.currentVersion = version;

    try {
      this.evaluate(version, lastSeen, hadPreviousData);
    } finally {
      this.ready = true;
    }
  }

  /** Marque la version courante comme vue et ferme la fenêtre. */
  dismiss() {
    writeLastSeen(this.currentVersion);
    this.open = false;
  }

  /** Ferme sans marquer : la fenêtre reviendra au prochain lancement. */
  later() {
    this.open = false;
  }

  /** Réouverture manuelle depuis les Paramètres. */
  showCurrent() {
    const release =
      releaseForVersion(this.currentVersion) ?? releasesSince(this.currentVersion, null)[0];
    if (!release) return;
    this.releases = [release];
    this.open = true;
  }

  private evaluate(version: string, lastSeen: string | null, hadPreviousData: boolean) {
    if (!lastSeen) {
      // Installation neuve : rien à annoncer, on mémorise simplement la version.
      if (!hadPreviousData) {
        writeLastSeen(version);
        return;
      }

      // Mise à jour depuis une version antérieure au changelog : on affiche
      // les entrées connues jusqu'à la version courante.
      const pending = releasesSince(version, null);
      if (pending.length === 0) {
        writeLastSeen(version);
        return;
      }
      this.releases = pending;
      this.open = true;
      return;
    }

    const comparison = compareVersions(version, lastSeen);
    if (comparison <= 0) {
      // Même version : rien. Retour arrière : on recale la clé sans modal.
      if (comparison < 0) writeLastSeen(version);
      return;
    }

    const pending = releasesSince(version, lastSeen);
    if (pending.length === 0) {
      // Version sans entrée de changelog : on évite de redemander à chaque lancement.
      writeLastSeen(version);
      return;
    }
    this.releases = pending;
    this.open = true;
  }
}

export const whatsNew = new WhatsNewStore();
