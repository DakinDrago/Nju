<script lang="ts">
  import Icon from "$lib/components/Icon.svelte";
  import { theme, type Theme } from "$lib/theme.svelte";

  const options: { value: Theme; label: string; icon: string }[] = [
    { value: "light", label: "Clair", icon: "sun" },
    { value: "dark", label: "Sombre", icon: "moon" },
    { value: "system", label: "Système", icon: "monitor" },
  ];
</script>

<div class="mx-auto max-w-2xl px-8 py-14">
  <h1 class="text-2xl font-semibold tracking-tight">Paramètres</h1>

  <section class="mt-8">
    <h2 class="text-sm font-medium">Thème</h2>
    <div class="mt-3 inline-flex gap-1 rounded-lg border border-border bg-surface p-1">
      {#each options as option}
        <button
          onclick={() => theme.set(option.value)}
          class={`flex items-center gap-2 rounded-md px-3 py-1.5 text-sm transition-colors ${
            theme.mode === option.value
              ? "bg-accent text-accent-foreground"
              : "text-muted hover:text-foreground"
          }`}
        >
          <Icon name={option.icon} class="size-4" />
          {option.label}
        </button>
      {/each}
    </div>
  </section>
</div>