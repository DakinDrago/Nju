import { writable, derived, get } from "svelte/store";
import { browser } from "$app/environment";

interface AudioDevice {
  deviceId: string;
  label: string;
  kind: "audioinput" | "audiooutput" | "videoinput";
}

function createMicrophoneStore() {
  const selected = writable<string>("");
  const devices = writable<AudioDevice[]>([]);
  const isLoading = writable(true);
  let watchCleanup: (() => void) | null = null;

  /** `silent` : rafraîchissement d'arrière-plan, sans afficher « Chargement... ». */
  async function loadDevices(silent = false) {
    if (!browser) return;
    if (!silent) isLoading.set(true);
    try {
      const allDevices = await navigator.mediaDevices.enumerateDevices();
      const audioInputs = allDevices.filter(d => d.kind === "audioinput") as AudioDevice[];
      devices.set(audioInputs);

      // Priorité : micro sauvegardé, puis sélection courante encore valide,
      // puis premier micro disponible.
      const saved = localStorage.getItem("selected-microphone");
      const current = get(selected);
      if (saved && audioInputs.some(d => d.deviceId === saved)) {
        selected.set(saved);
      } else if (current && audioInputs.some(d => d.deviceId === current)) {
        // La sélection courante existe toujours : on la garde.
      } else if (audioInputs.length > 0) {
        // Default to first device
        selected.set(audioInputs[0].deviceId);
      }
    } catch (e) {
      console.error("Failed to load audio devices:", e);
    } finally {
      if (!silent) isLoading.set(false);
    }
  }

  function selectDevice(deviceId: string) {
    selected.set(deviceId);
    if (browser) {
      localStorage.setItem("selected-microphone", deviceId);
    }
  }

  const selectedDevice = derived(
    [devices, selected],
    ([$devices, $selected]) => ($devices ?? []).find(d => d.deviceId === $selected) ?? null
  );

  /**
   * Réénumère les micros quand le matériel change, quand la fenêtre reprend le
   * focus ou redevient visible (retour de tray). Retourne une fonction d'arrêt.
   *
   * `devicechange` est le mécanisme principal ; focus/visibilité couvrent les
   * plateformes où l'événement est manqué (certaines configs Linux/macOS).
   */
  function startWatching() {
    if (!browser) return () => {};
    if (watchCleanup) return watchCleanup;

    const media = navigator.mediaDevices;
    if (!media) return () => {};

    let timer: ReturnType<typeof setTimeout> | undefined;
    const refresh = (delay = 0) => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = undefined;
        void loadDevices(true);
      }, delay);
    };
    const onDeviceChange = () => refresh(250);
    const onFocus = () => refresh();
    const onVisibility = () => {
      if (document.visibilityState === "visible") refresh();
    };

    media.addEventListener("devicechange", onDeviceChange);
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibility);

    const stop = () => {
      if (timer) clearTimeout(timer);
      timer = undefined;
      media.removeEventListener("devicechange", onDeviceChange);
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibility);
      watchCleanup = null;
    };
    watchCleanup = stop;
    return stop;
  }

  return {
    subscribe: selected.subscribe,
    devices,
    isLoading,
    selectedDevice,
    loadDevices,
    selectDevice,
    startWatching,
  };
}

export const microphone = createMicrophoneStore();
