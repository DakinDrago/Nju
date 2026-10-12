import { writable, derived } from "svelte/store";
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

  async function loadDevices() {
    if (!browser) return;
    isLoading.set(true);
    try {
      const allDevices = await navigator.mediaDevices.enumerateDevices();
      const audioInputs = allDevices.filter(d => d.kind === "audioinput") as AudioDevice[];
      devices.set(audioInputs);
      
      // Load saved preference
      const saved = localStorage.getItem("selected-microphone");
      if (saved && audioInputs.some(d => d.deviceId === saved)) {
        selected.set(saved);
      } else if (audioInputs.length > 0) {
        // Default to first device
        selected.set(audioInputs[0].deviceId);
      }
    } catch (e) {
      console.error("Failed to load audio devices:", e);
    } finally {
      isLoading.set(false);
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

  return {
    subscribe: selected.subscribe,
    devices,
    isLoading,
    selectedDevice,
    loadDevices,
    selectDevice,
  };
}

export const microphone = createMicrophoneStore();