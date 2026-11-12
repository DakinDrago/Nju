/**
 * Contenu des fenêtres « Nouveautés ».
 *
 * Ajouter une entrée en tête de tableau à chaque version publiée. Le champ
 * `version` doit correspondre exactement à la version de l'application
 * (`package.json` / `tauri.conf.json`). Les sections affichées sont celles
 * qui intéressent l'utilisateur ; les détails internes restent dans les
 * commits et les notes de release.
 */
export type ChangelogSection = {
  title: string;
  items: string[];
};

export type ChangelogRelease = {
  version: string;
  /** Date ISO (AAAA-MM-JJ). */
  date: string;
  sections: ChangelogSection[];
};

export const RELEASES: ChangelogRelease[] = [
  {
    version: "0.2.2",
    date: "2026-11-12",
    sections: [
      {
        title: "Ajouté",
        items: [
          "Fenêtre « Nouveautés » affichée une fois après chaque mise à jour, avec accès manuel depuis les Paramètres.",
          "Support du GPU DirectML sous Windows, avec repli automatique sur le processeur si le GPU ou le pilote n'est pas compatible.",
          "Détection automatique des microphones branchés ou débranchés, avec bouton Actualiser et réacquisition du flux avant chaque enregistrement.",
          "Mode sombre complet : notifications, page Modèles et affichage sans flash clair au lancement.",
          "Blocage des raccourcis de rechargement (F5, Ctrl+R) en version installée, sans affecter le copier/coller ni la saisie.",
        ],
      },
      {
        title: "Corrigé",
        items: [
          "Le rechargement de l'interface ne décharge plus le modèle local.",
          "Les interrupteurs des Paramètres sont de nouveau visibles et animés, en clair comme en sombre.",
          "Le sélecteur de microphone ne déborde plus et le bouton Actualiser ne saute plus pendant le chargement.",
          "Le statut du modèle et l'emplacement du bouton Annuler restent stables pendant l'enregistrement.",
          "Les badges de la page Modèles sont lisibles dans les deux thèmes.",
          "Correction du démarrage sous Linux (pile non exécutable et renderer WebKitGTK).",
        ],
      },
      {
        title: "Modifié",
        items: [
          "ONNX Runtime Windows embarque désormais DirectML 1.15.4 pour les GPU compatibles DirectX 12.",
          "Les réglages de périphérique et de threads ne rechargent plus le modèle inutilement.",
          "Libellés de statut en français et mise en page harmonisée.",
        ],
      },
    ],
  },
];

/** Compare deux versions numériques (`0.2.2`, `v0.2.2`, `0.3`). */
export function compareVersions(a: string, b: string): number {
  const parse = (value: string): number[] =>
    value
      .replace(/^v/i, "")
      .split(/[.+-]/)
      .map((part) => {
        const n = Number.parseInt(part, 10);
        return Number.isFinite(n) ? n : 0;
      });

  const pa = parse(a);
  const pb = parse(b);
  const length = Math.max(pa.length, pb.length);
  for (let i = 0; i < length; i++) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

/** Versions à présenter : `lastSeen` exclu, `current` inclus, plus récentes d'abord. */
export function releasesSince(current: string, lastSeen: string | null): ChangelogRelease[] {
  return RELEASES.filter((release) => {
    if (compareVersions(release.version, current) > 0) return false;
    if (lastSeen && compareVersions(release.version, lastSeen) <= 0) return false;
    return true;
  }).sort((a, b) => compareVersions(b.version, a.version));
}

/** Entrée correspondant exactement à une version, si elle existe. */
export function releaseForVersion(version: string): ChangelogRelease | undefined {
  return RELEASES.find((release) => compareVersions(release.version, version) === 0);
}
