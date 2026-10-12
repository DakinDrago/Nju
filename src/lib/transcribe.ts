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

/** Convertit un blob audio (WebM/OPUS) en WAV 16-bit mono 16kHz pour l'API de transcription.
    Retourne aussi la durée réelle mesurée sur l'audio décodé (fiable, indépendante du conteneur). */
async function convertToWav16kHz(blob: Blob): Promise<{ wav: Blob; durationSec: number }> {
  const arrayBuffer = await blob.arrayBuffer();
  
  // 1. Décode le fichier source (WebM/OPUS) à sa fréquence native
  const audioCtx = new AudioContext();
  let audioBuffer;
  try {
    audioBuffer = await audioCtx.decodeAudioData(arrayBuffer);
  } catch (e) {
    await audioCtx.close();
    throw new Error("Impossible de décoder l'audio: " + String(e));
  }
  await audioCtx.close();
  
  if (!audioBuffer || audioBuffer.duration <= 0 || audioBuffer.length <= 0) {
    throw new Error("Audio buffer invalide (durée: " + audioBuffer?.duration + ", longueur: " + audioBuffer?.length + ")");
  }
  
  console.log("Audio décodé:", { duration: audioBuffer.duration, sampleRate: audioBuffer.sampleRate, channels: audioBuffer.numberOfChannels, length: audioBuffer.length });
  
  // 2. Rééchantillonne à 16kHz mono via OfflineAudioContext
  const targetSamples = Math.ceil(audioBuffer.duration * 16000);
  console.log("OfflineAudioContext target samples:", targetSamples);
  
  let resampledBuffer;
  try {
    const offlineCtx = new OfflineAudioContext(1, Math.ceil(audioBuffer.duration * 16000), 16000);
    const bufferSource = offlineCtx.createBufferSource();
    bufferSource.buffer = audioBuffer;
    bufferSource.connect(offlineCtx.destination);
    bufferSource.start(0);
    
    resampledBuffer = await offlineCtx.startRendering();
    console.log("OfflineAudioContext rendu:", { duration: resampledBuffer.duration, sampleRate: resampledBuffer.sampleRate, length: resampledBuffer.length });
  } catch (e) {
    throw new Error("Échec OfflineAudioContext: " + String(e));
  }
  
  if (!resampledBuffer || resampledBuffer.length <= 0) {
    throw new Error("OfflineAudioContext a retourné un buffer vide (longueur: " + resampledBuffer?.length + ")");
  }
  
  // 3. Encode WAV 16-bit mono 16kHz
  const length = resampledBuffer.length;
  const wavLength = 44 + length * 2;
  const view = new DataView(new ArrayBuffer(wavLength));
  const writeString = (offset: number, s: string) => {
    for (let i = 0; i < s.length; i++) view.setUint8(offset + i, s.charCodeAt(i));
  };
  
  const monoData = resampledBuffer.getChannelData(0);
  
  writeString(0, "RIFF");
  view.setUint32(4, 36 + length * 2, true);
  writeString(8, "WAVE");
  writeString(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, 16000, true);
  view.setUint32(28, 32000, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeString(36, "data");
  view.setUint32(40, length * 2, true);
  
  let offset = 44;
  for (let i = 0; i < length; i++, offset += 2) {
    const s = Math.max(-1, Math.min(1, monoData[i]));
    view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7fff, true);
  }
  
  // Validation du header AVANT création du blob (utilise l'ArrayBuffer original)
  const wavArrayBuffer = view.buffer;
  const valView = new DataView(wavArrayBuffer);
  const riff = String.fromCharCode(valView.getUint8(0), valView.getUint8(1), valView.getUint8(2), valView.getUint8(3));
  const wave = String.fromCharCode(valView.getUint8(8), valView.getUint8(9), valView.getUint8(10), valView.getUint8(11));
  const fmt = String.fromCharCode(valView.getUint8(12), valView.getUint8(13), valView.getUint8(14), valView.getUint8(15));
  console.log("WAV header validation:", { riff, wave, fmt, fileSize: valView.getUint32(4, true), audioFormat: valView.getUint16(20, true), channels: valView.getUint16(22, true), sampleRate: valView.getUint32(24, true), bitsPerSample: valView.getUint16(34, true), dataSize: valView.getUint32(40, true) });
  
  if (riff !== "RIFF" || wave !== "WAVE" || fmt !== "fmt ") {
    throw new Error("WAV header invalide après génération");
  }
  
  const wavBlob = new Blob([view], { type: "audio/wav" });
  console.log("WAV généré:", { size: wavBlob.size, type: wavBlob.type });
  return { wav: wavBlob, durationSec: audioBuffer.duration };
}

/** Déduit une extension de fichier sûre depuis le type MIME (pour l'historique). */
function extensionForMime(mime: string): string {
  const type = mime.split(";")[0].trim().toLowerCase();
  switch (type) {
    case "audio/webm": return "webm";
    case "audio/ogg": return "ogg";
    case "audio/opus": return "opus";
    case "audio/wav":
    case "audio/x-wav":
    case "audio/wave": return "wav";
    case "audio/mpeg":
    case "audio/mp3": return "mp3";
    case "audio/mp4":
    case "audio/x-m4a": return "m4a";
    case "audio/flac": return "flac";
    case "audio/aiff":
    case "audio/x-aiff": return "aiff";
    default: return "opus";
  }
}

/** Point d'entrée unique de l'interface : elle ne sait pas quel moteur répond.
    `chunkSecs` (local) et `timeoutSecs` (serveur) viennent des Paramètres. */
export async function transcribe(
  audioBlob: Blob,
  opts?: { extension?: string; chunkSecs?: number; timeoutSecs?: number },
): Promise<string> {
  const engine = engines.active;
  if (!engine) throw new Error("Aucun moteur sélectionné (écran Modèles)");
  
  console.log("Transcription - audioBlob:", { size: audioBlob.size, type: audioBlob.type });
  
  // Convertit en WAV 16kHz mono pour l'API de transcription
  let wavBlob: Blob;
  let duration: number;
  try {
    const converted = await convertToWav16kHz(audioBlob);
    wavBlob = converted.wav;
    duration = converted.durationSec;
  } catch (e) {
    console.error("Échec conversion WAV:", e);
    throw new Error("Impossible de convertir l'audio en WAV 16kHz: " + String(e));
  }
  
  const audioB64 = await blobToBase64(wavBlob);
  const sampleRate = 16000;

  let text: string;
  if (engine.kind === "local") {
    text = await invoke<string>("transcribe_local", {
      model: folderOf(engine.id),
      audioB64,
      chunkSecs: opts?.chunkSecs ?? 25,
    });
  } else {
    text = await invoke<string>("transcribe_server", {
      config: toConfig(engine),
      audioB64,
      timeoutSecs: opts?.timeoutSecs ?? 120,
    });
  }

  // Sauvegarde dans l'historique (garde le format original)
  const engineLabel = engine.kind === "local" ? "local:" + engine.id : "server:" + engine.id;
  try {
    await invoke<string>("save_transcription", {
      audioB64: await blobToBase64(audioBlob), // Format original pour l'historique
      originalText: text,
      engine: engineLabel,
      durationSec: duration,
      sampleRate,
      extension: opts?.extension ?? extensionForMime(audioBlob.type),
    });
  } catch (e) {
    console.warn("Échec sauvegarde historique:", e);
  }

  return text;
}