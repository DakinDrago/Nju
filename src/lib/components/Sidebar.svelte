<script lang="ts">
  import { page } from "$app/state";
  import { Button } from "$lib/components/ui/button";
  import { Mic, History, Cpu, SlidersHorizontal } from "lucide-svelte";
  import { cn } from "$lib/utils";

  const items = [
    { href: "/", label: "Nouveau", icon: Mic },
    { href: "/historique", label: "Historique", icon: History },
    { href: "/modeles", label: "Modèles", icon: Cpu },
    { href: "/parametres", label: "Paramètres", icon: SlidersHorizontal },
  ];

  const isActive = (href: string) =>
    href === "/" ? page.url.pathname === "/" : page.url.pathname.startsWith(href);
</script>

<aside class="flex w-56 shrink-0 flex-col border-r border-border bg-card px-3 py-5">
  <div class="px-3 pb-6 text-lg font-semibold tracking-tight">Njū'</div>

  <nav class="flex flex-col gap-1">
    {#each items as item}
      <a
        href={item.href}
        class={cn(
          "flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors",
          isActive(item.href)
            ? "bg-primary/10 text-primary"
            : "text-muted-foreground hover:bg-muted hover:text-foreground"
        )}
      >
        <item.icon class="size-4" />
        {item.label}
      </a>
    {/each}
  </nav>
</aside>