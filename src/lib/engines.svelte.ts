import { invoke } from "@tauri-apps/api/core";
import { settings } from "./settings.svelte";

export type ServerEngine = {
  id: string;
  name: string;
  url: string;
  apiKey: string;
  model: string;
  language: string;
  insecure: boolean;
};

/** Modèle ONNX installé dans le dossier « models » de l'appli (id = nom du sous-dossier). */
export type LocalModel = { id: string; name: string; sizeMb: number };

export type ActiveEngine =
  | ({ kind: "server" } & ServerEngine)
  | { kind: "local"; id: string; name: string };

export type Device = "auto" | "gpu" | "cpu";

/** Ce que le binaire sait faire (voir `get_capabilities` côté Rust). */
export type Capabilities = {
  gpu: boolean;
  platform: string;
};

export type EngineState = "unknown" | "checking" | "online" | "offline";
export type Status = { state: EngineState; message: string };

export const dotClass: Record<EngineState, string> = {
  online: "bg-emerald-500",
  offline: "bg-danger",
  checking: "bg-amber-400 animate-pulse",
  unknown: "bg-muted/50",
};

const KEY = "nju-engines";
const LOCAL = "local:";

export const localKey = (folder: string) => LOCAL + folder;
export const isLocalKey = (id: string | null): id is string => !!id && id.startsWith(LOCAL);
export const folderOf = (id: string) => id.slice(LOCAL.length);

const newId = () => crypto.randomUUID?.() ?? Math.random().toString(36).slice(2);

// Format attendu par la commande Rust (champs en camelCase)
export function toConfig(s: ServerEngine) {
  return {
    url: s.url,
    apiKey: s.apiKey || null,
    model: s.model || null,
    language: s.language || null,
    insecure: s.insecure,
  };
}

class EngineStore {
  servers = $state<ServerEngine[]>([]);
  locals = $state<LocalModel[]>([]);
  modelsDir = $state("");
  activeId = $state<string | null>(null);
  device = $state<Device>("auto");
  /** Threads CPU imposés (0 = auto). Appliqués au prochain chargement du modèle. */
  cpuThreads = $state(0);
  status = $state<Record<string, Status>>({});
  /** Dernier message d'échec par modèle local (persiste au redémarrage). */
  lastError = $state<Record<string, string>>({});
  caps = $state<Capabilities>({ gpu: false, platform: "" });

  get active(): ActiveEngine | null {
    if (isLocalKey(this.activeId)) {
      const m = this.locals.find((l) => l.id === folderOf(this.activeId!));
      return m ? { kind: "local", id: this.activeId, name: m.name } : null;
    }
    const s = this.servers.find((x) => x.id === this.activeId);
    return s ? { kind: "server", ...s } : null;
  }

  async init() {
    try {
      const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
      if (saved?.servers?.length) {
        this.servers = saved.servers;
        this.activeId = saved.activeId ?? saved.servers[0].id;
      }
      if (saved?.device && ["auto", "gpu", "cpu"].includes(saved.device)) {
        this.device = saved.device;
      } else if (saved?.device) {
        // Valeur inconnue : repli sur Auto.
        this.device = "auto";
      }
      if (typeof saved?.cpuThreads === "number" && Number.isFinite(saved.cpuThreads)) {
        this.cpuThreads = Math.min(64, Math.max(0, Math.round(saved.cpuThreads)));
      }
      if (saved?.lastError && typeof saved.lastError === "object") {
        this.lastError = saved.lastError;
      }
    } catch {}

    if (this.servers.length === 0) {
      const id = newId();
      this.servers = [
        { id, name: "Serveur local", url: "http://127.0.0.1:8000", apiKey: "", model: "", language: "", insecure: false },
      ];
      this.activeId = id;
      this.save();
    }

    try {
      this.caps = await invoke<Capabilities>("get_capabilities");
    } catch (e) {
      console.error(e);
    }
    // Si le mode sauvegardé n'est plus possible dans ce build, retour à Auto.
    if (this.device === "gpu" && !this.caps.gpu) this.device = "auto";

    try {
      await invoke("set_local_device", { device: this.device });
    } catch (e) {
      console.error(e);
    }
    try {
      await invoke("set_cpu_threads", { threads: this.cpuThreads });
    } catch (e) {
      console.error(e);
    }
    await this.refreshLocals();

    // Le modèle local actif a disparu (dossier supprimé) : retour au premier serveur
    if (isLocalKey(this.activeId) && !this.active) {
      this.activeId = this.servers[0]?.id ?? null;
      this.save();
    }
    if (this.activeId) this.check(this.activeId);
  }

  async refreshLocals() {
    try {
      this.modelsDir = await invoke<string>("models_dir");
      this.locals = await invoke<LocalModel[]>("list_local_models");
    } catch (e) {
      console.error(e);
    }
  }

  private save() {
    try {
      localStorage.setItem(KEY, JSON.stringify({ servers: this.servers, activeId: this.activeId, device: this.device, cpuThreads: this.cpuThreads, lastError: this.lastError }));
    } catch {}
  }

  add(data: Omit<ServerEngine, "id">) {
    const id = newId();
    this.servers.push({ id, ...data });
    if (!this.activeId) this.activeId = id;
    this.save();
    this.test(id);
  }

  update(id: string, data: Omit<ServerEngine, "id">) {
    const i = this.servers.findIndex((s) => s.id === id);
    if (i === -1) return;
    this.servers[i] = { id, ...data };
    this.save();
    this.test(id);
  }

  remove(id: string) {
    this.servers = this.servers.filter((s) => s.id !== id);
    delete this.status[id];
    if (this.activeId === id) {
      this.activeId = this.servers[0]?.id ?? (this.locals[0] ? localKey(this.locals[0].id) : null);
    }
    this.save();
  }

  async setActive(id: string) {
    const prev = this.activeId;
    this.activeId = id;
    this.save();
    // Quitter un modèle local libère sa mémoire (le slot ne garde qu'un modèle).
    if (isLocalKey(prev) && prev !== id) {
      try {
        await invoke("unload_local_model", { model: null });
      } catch (e) {
        console.error(e);
      }
      if (this.status[prev]?.state === "online") {
        this.status[prev] = { state: "unknown", message: "Non chargé" };
      }
    }
    await this.check(id);
  }

  /** Décharge un modèle local précis (libère la RAM). Sans effet s'il n'est pas chargé. */
  async unmount(id: string) {
    try {
      const wasLoaded = await invoke<boolean>("unload_local_model", { model: folderOf(id) });
      if (wasLoaded) this.status[id] = { state: "unknown", message: "Non chargé" };
    } catch (e) {
      console.error(e);
    }
  }

  /** « auto » : GPU si disponible, sinon CPU ; « gpu » : GPU requis ; « cpu » : CPU imposé. Recharge le modèle local actif. */
  async setDevice(device: Device) {
    this.device = device;
    this.save();
    try {
      await invoke("set_local_device", { device });
    } catch (e) {
      console.error(e);
    }
    if (isLocalKey(this.activeId)) this.load(this.activeId);
  }

  /** Threads CPU (0 = auto, sinon 1–64). Recharge le modèle local actif. */
  async setCpuThreads(threads: number) {
    this.cpuThreads = Math.min(64, Math.max(0, Math.round(threads)));
    this.save();
    try {
      await invoke("set_cpu_threads", { threads: this.cpuThreads });
    } catch (e) {
      console.error(e);
    }
    if (isLocalKey(this.activeId)) this.load(this.activeId);
  }

  /** Importe un dossier de modèle exporté, puis le sélectionne. */
  async importModel(source: string, mode: "copy" | "link") {
    const name = await invoke<string>("import_local_model", { source, mode });
    await this.refreshLocals();
    this.setActive(localKey(name));
    return name;
  }

  async removeLocal(folder: string) {
    const id = localKey(folder);
    await invoke("remove_local_model", { model: folder });
    delete this.status[id];
    delete this.lastError[id];
    this.save();
    await this.refreshLocals();
    if (this.activeId === id) {
      this.activeId = this.servers[0]?.id ?? null;
      this.save();
      if (this.activeId) this.check(this.activeId);
    }
  }

  /** Serveur : teste la connexion. Modèle local : charge le modèle en mémoire. */
  check(id: string) {
    return isLocalKey(id) ? this.load(id) : this.test(id);
  }

  async test(id: string) {
    const s = this.servers.find((x) => x.id === id);
    if (!s) return;
    this.status[id] = { state: "checking", message: "Test en cours..." };
    try {
      const message = await invoke<string>("test_server", {
        config: toConfig(s),
        timeoutSecs: settings.data.serverTestTimeout,
      });
      this.status[id] = { state: "online", message };
    } catch (e) {
      this.status[id] = { state: "offline", message: String(e) };
    }
  }

  async load(id: string) {
    const folder = folderOf(id);
    const m = this.locals.find((l) => l.id === folder);
    const big = m && m.sizeMb >= 1500 ? `Gros modèle (~${(m.sizeMb / 1000).toFixed(1)} Go) — ` : "";
    this.status[id] = { state: "checking", message: `${big}Chargement du modèle...` };
    try {
      const message = await invoke<string>("load_local_model", { model: folder });
      this.status[id] = { state: "online", message };
      if (this.lastError[id]) {
        delete this.lastError[id];
        this.save();
      }
    } catch (e) {
      const message = String(e);
      this.status[id] = { state: "offline", message };
      this.lastError[id] = message;
      this.save();
    }
  }
}

export const engines = new EngineStore();