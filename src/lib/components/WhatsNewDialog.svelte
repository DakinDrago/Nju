<script lang="ts">
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
  } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Sparkles, Wrench, SlidersHorizontal } from "lucide-svelte";
  import { whatsNew } from "$lib/whats-new.svelte";

  const sectionIcons: Record<string, typeof Sparkles> = {
    Ajouté: Sparkles,
    Corrigé: Wrench,
    Modifié: SlidersHorizontal,
  };

  function formatDate(iso: string): string {
    // Midi UTC pour éviter un décalage de jour selon le fuseau.
    const date = new Date(`${iso}T12:00:00`);
    return date.toLocaleDateString("fr-FR", {
      day: "numeric",
      month: "long",
      year: "numeric",
    });
  }
</script>

<Dialog bind:open={whatsNew.open}>
  <DialogContent class="sm:max-w-lg">
    <DialogHeader>
      <div class="flex items-center gap-3">
        <div class="flex size-9 shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary">
          <Sparkles class="size-4" />
        </div>
        <div class="min-w-0">
          <DialogTitle>Nouveautés</DialogTitle>
          <DialogDescription class="text-xs">
            {#if whatsNew.releases.length === 1}
              Version {whatsNew.releases[0].version} · {formatDate(whatsNew.releases[0].date)}
            {:else}
              {whatsNew.releases.length} versions à découvrir
            {/if}
          </DialogDescription>
        </div>
      </div>
    </DialogHeader>

    <div class="-mx-1 max-h-[65vh] space-y-6 overflow-y-auto px-1">
      {#each whatsNew.releases as release (release.version)}
        <section class="space-y-3">
          {#if whatsNew.releases.length > 1}
            <div class="flex items-baseline justify-between gap-3 border-b border-border pb-1.5">
              <h3 class="text-sm font-semibold">Version {release.version}</h3>
              <span class="shrink-0 text-xs text-muted-foreground">{formatDate(release.date)}</span>
            </div>
          {/if}

          {#each release.sections as section}
            {@const Icon = sectionIcons[section.title] ?? Sparkles}
            <div class="space-y-1.5">
              <h4 class="flex items-center gap-1.5 text-xs font-medium tracking-wide text-muted-foreground uppercase">
                <Icon class="size-3.5" />
                {section.title}
              </h4>
              <ul class="space-y-1.5">
                {#each section.items as item}
                  <li class="flex gap-2 text-sm leading-relaxed">
                    <span class="mt-2 size-1 shrink-0 rounded-full bg-primary/60"></span>
                    <span>{item}</span>
                  </li>
                {/each}
              </ul>
            </div>
          {/each}
        </section>
      {/each}
    </div>

    <DialogFooter class="gap-2 bg-transparent">
      <Button variant="ghost" onclick={() => whatsNew.later()}>Plus tard</Button>
      <Button onclick={() => whatsNew.dismiss()}>J'ai compris</Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
