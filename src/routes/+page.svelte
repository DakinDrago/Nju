<script lang="ts">
  import { onMount } from "svelte";
  import { engines, dotClass } from "$lib/engines.svelte";
  import { transcribe } from "$lib/transcribe";

  const engineState = $derived(
    engines.active ? (engines.status[engines.active.id]?.state ?? "unknown") : "unknown",
  );

  let status = $state("Prêt");
  let result = $state("");
  let recording = $state(false);

  let stream: MediaStream | null = null;
  let ctx: AudioContext | null = null;
  let chunks: Float32Array[] = [];

  onMount(() => {
    navigator.mediaDevices
      .getUserMedia({ audio: true })
      .then((s) => (stream = s))
      .catch((err) => {
        status = "Impossible d'accéder au microphone";
        console.error(err);
      });

    return () => {
      stream?.getTracks().forEach((t) => t.stop());
      ctx?.close();
    };
  });

  // Capture le PCM brut (pas de MediaRecorder : les formats varient selon la webview)
  async function start() {
    if (!stream || recording) return;
    if (!ctx) {
      ctx = new AudioContext();
      const source = ctx.createMediaStreamSource(stream);
      const processor = ctx.createScriptProcessor(4096, 1, 1);
      processor.onaudioprocess = (e) => {
        if (recording) chunks.push(new Float32Array(e.inputBuffer.getChannelData(0)));
      };
      source.connect(processor);
      processor.connect(ctx.destination);
    }
    await ctx.resume();
    chunks = [];
    recording = true;
    status = "Enregistrement...";
  }

  async function stop() {
    if (!recording || !ctx) return;
    recording = false;

    const samples = chunks.reduce((n, c) => n + c.length, 0);
    if (samples < ctx.sampleRate * 0.3) {
      status = "Trop court, maintiens plus longtemps";
      return;
    }

    status = "Transcription en cours...";
    try {
      result = await transcribe(encodeWav(chunks, ctx.sampleRate));
      status = "Terminé";
    } catch (err) {
      result = "Erreur : " + String(err);
      status = "Erreur";
      console.error(err);
    }
  }

  // WAV 16 bits mono ; le backend rééchantillonne à 16 kHz
  function encodeWav(parts: Float32Array[], sampleRate: number): Blob {
    const length = parts.reduce((n, c) => n + c.length, 0);
    const view = new DataView(new ArrayBuffer(44 + length * 2));
    const text = (offset: number, s: string) =>
      [...s].forEach((ch, i) => view.setUint8(offset + i, ch.charCodeAt(0)));

    text(0, "RIFF");
    view.setUint32(4, 36 + length * 2, true);
    text(8, "WAVEfmt ");
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true); // PCM
    view.setUint16(22, 1, true); // mono
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * 2, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    text(36, "data");
    view.setUint32(40, length * 2, true);

    let offset = 44;
    for (const part of parts) {
      for (let i = 0; i < part.length; i++, offset += 2) {
        const s = Math.max(-1, Math.min(1, part[i]));
        view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7fff, true);
      }
    }
    return new Blob([view], { type: "audio/wav" });
  }
</script>

<div class="mx-auto flex min-h-full max-w-2xl flex-col items-center px-8 py-14 text-center">
  <h1 class="text-2xl font-semibold tracking-tight">Nouvelle transcription</h1>
  <p class="mt-2 mb-5 text-sm text-muted">Maintiens le bouton pour parler</p>

  <a
    href="/modeles"
    class="mb-10 inline-flex items-center gap-2 rounded-full border border-border bg-surface px-3 py-1 text-xs text-muted transition-colors hover:text-foreground"
  >
    <span class={`size-2 rounded-full ${dotClass[engineState]}`}></span>
    {engines.active?.name ?? "Aucun moteur"}
  </a>

  <button
    class={`grid size-36 touch-none place-items-center rounded-full text-base font-medium shadow-lg transition select-none ${
      recording
        ? "scale-105 bg-danger text-white shadow-danger/40"
        : "bg-accent text-accent-foreground shadow-accent/30"
    }`}
    onpointerdown={(e) => {
      e.currentTarget.setPointerCapture(e.pointerId);
      start();
    }}
    onpointerup={stop}
    onpointercancel={stop}
    oncontextmenu={(e) => e.preventDefault()}
  >
    <span>
      {#if recording}Parle...<br />(relâche){:else}Maintenir<br />pour parler{/if}
    </span>
  </button>

  <p class="mt-6 min-h-6 text-sm text-muted">{status}</p>

  <section class="mt-10 w-full text-left">
    <div class="mb-2 text-xs font-medium tracking-wide text-muted uppercase">Transcription</div>
    <div
      class={`min-h-28 rounded-xl border border-border bg-surface p-5 text-base leading-relaxed shadow-sm ${
        result ? "" : "text-muted"
      }`}
    >
      {result || "La transcription apparaîtra ici..."}
    </div>
  </section>
</div>