<script lang="ts">
  import { page } from "$app/state";
  import Icon from "./Icon.svelte";

  const items = [
    { href: "/", label: "Nouveau", icon: "mic" },
    { href: "/historique", label: "Historique", icon: "history" },
    { href: "/modeles", label: "Modèles", icon: "cpu" },
    { href: "/parametres", label: "Paramètres", icon: "sliders" },
  ];

  const isActive = (href: string) =>
    href === "/" ? page.url.pathname === "/" : page.url.pathname.startsWith(href);
</script>

<aside class="flex w-56 shrink-0 flex-col border-r border-border bg-surface px-3 py-5">
  <div class="px-3 pb-6 text-lg font-semibold tracking-tight">Njū'</div>

  <nav class="flex flex-col gap-1">
    {#each items as item}
      <a
        href={item.href}
        class={`flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
          isActive(item.href)
            ? "bg-accent/10 text-accent"
            : "text-muted hover:bg-border/50 hover:text-foreground"
        }`}
      >
        <Icon name={item.icon} class="size-4" />
        {item.label}
      </a>
    {/each}
  </nav>
</aside>