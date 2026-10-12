import { invoke } from "@tauri-apps/api/core";
import { engines, folderOf, toConfig } from "./engines.svelte";

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve((reader.result as string).split(",")[1]);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/** Point d'entrée unique de l'interface : elle ne sait pas quel moteur répond. */
export async function transcribe(wav: Blob): Promise<string> {
  const engine = engines.active;
  if (!engine) throw new Error("Aucun moteur sélectionné (écran Modèles)");
  const audioB64 = await blobToBase64(wav);

  if (engine.kind === "local") {
    return await invoke<string>("transcribe_local", { model: folderOf(engine.id), audioB64 });
  }
  return await invoke<string>("transcribe_server", { config: toConfig(engine), audioB64 });
}