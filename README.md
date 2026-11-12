# Njū'

[![Dernière version](https://img.shields.io/github/v/release/DakinDrago/Nju?label=version&sort=semver)](https://github.com/DakinDrago/Nju/releases/latest)
[![Licence : GPL-3.0](https://img.shields.io/badge/licence-GPL--3.0-blue.svg)](LICENSE)
[![Plateformes](https://img.shields.io/badge/plateformes-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey.svg)](#plateformes)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)

**Transcription audio locale ou par serveur, pensée pour le fe'efe'e (`fmp`).**

Njū' (`to-nju`) enregistre ou importe de l'audio, le transcrit, permet de corriger
le texte et exporte un corpus prêt pour l'entraînement de MMS-1B-all. Tout peut
fonctionner hors ligne avec un modèle ONNX local ; un serveur compatible OpenAI
peut aussi être utilisé.

## Fonctionnalités

- **Enregistrement micro** avec raccourci global configurable
  (`Ctrl+Shift+Space` par défaut), utilisable depuis n'importe quelle application.
- **Import audio** par glisser-déposer ou sélection de fichier.
- **Deux moteurs** : modèle local ONNX (wav2vec2/MMS) ou serveur compatible
  OpenAI (`POST /v1/audio/transcriptions`).
- **Accélération matérielle** : DirectML sous Windows, WebGPU sous Linux,
  CoreML sous macOS, avec repli automatique sur le CPU.
- **Historique** : audio, texte original, texte corrigé, statut et recherche.
- **Export MMS-1B-all** au format TSV, avec copie des audios et fichier
  récapitulatif.
- **Interface** claire/sombre/système, icône de notification, fermeture en
  arrière-plan et instance unique.
- **Mises à jour signées** et fenêtre « Nouveautés » affichée une fois par
  version.

## Plateformes

| Plateforme | Paquets | Accélération |
| --- | --- | --- |
| Windows 10/11 x64 | NSIS `.exe` | DirectML, CPU |
| Linux x64 | AppImage, `.deb`, `.rpm` | WebGPU, CPU |
| macOS Apple Silicon | `.pkg`, `.dmg` | CoreML, CPU |

> Les binaires ne sont pas encore signés. Windows SmartScreen et macOS
> Gatekeeper peuvent donc afficher un avertissement au premier lancement.

## Installation

Télécharge le paquet correspondant à ton système depuis la
[dernière release](https://github.com/DakinDrago/Nju/releases/latest).

### Windows

Lance `to-nju_<version>_x64-setup.exe` et suis l'assistant. Si SmartScreen
bloque l'exécution, clique sur **Informations complémentaires**, puis
**Exécuter quand même**.

### Linux

- **AppImage** : rends le fichier exécutable (`chmod +x`) puis lance-le.
- **Debian/Ubuntu** : `sudo apt install ./to-nju_<version>_amd64.deb`
- **Fedora/RHEL** : `sudo rpm -i to-nju-<version>-1.x86_64.rpm`

### macOS

- **`.pkg`** : installe l'application dans `/Applications`.
- **`.dmg`** : ouvre l'image et glisse `to-nju.app` dans le dossier
  **Applications**.

Le paquet `.pkg` n'est pas signé. Si Gatekeeper le refuse, autorise l'ouverture
dans **Réglages Système → Confidentialité et sécurité**, ou installe via le
`.dmg` et fais un clic droit sur l'application → **Ouvrir**.

### Premier lancement

Aucun modèle n'est fourni avec l'application. Il faut importer un modèle local
ou configurer un serveur avant de pouvoir transcrire (voir ci-dessous).

## Démarrage rapide

1. Ouvre **Modèles**.
2. Importe un dossier de modèle exporté (`model.onnx`, `vocab.json`,
   `meta.json`) ou ajoute un serveur compatible OpenAI.
3. Clique sur **Charger**.
4. Dans **Nouveau**, enregistre avec le bouton ou le raccourci global, ou
   dépose un fichier audio.
5. Corrige la transcription dans **Historique**.
6. Exporte le dataset MMS depuis **Historique**.

## Les moteurs

### Modèle local ONNX

Le moteur local exécute un modèle wav2vec2/MMS exporté en ONNX, sans Python ni
serveur. Un modèle est un sous-dossier du dossier `models/` de l'application et
doit contenir :

```text
mms-fmp/
├── model.onnx
├── model.onnx.data   # optionnel, pour les modèles à poids externes
├── vocab.json
└── meta.json
```

Ces fichiers sont produits par `export_onnx.py`, le script d'export
wav2vec2/MMS vers ONNX.

À l'import, deux modes sont proposés :

- **Copier** : le dossier est copié dans les données de l'application.
- **Lier** : aucun fichier n'est copié, mais le dossier d'origine doit rester
  en place. Ce mode n'est pas disponible sous Windows.

Les longs audios sont découpés automatiquement. La progression est affichée et
la transcription peut être annulée.

### Serveur compatible OpenAI

Le moteur serveur envoie l'audio à un endpoint `POST /v1/audio/transcriptions`
compatible OpenAI. Champs configurables :

- URL du serveur ;
- clé API ;
- nom du modèle ;
- langue ;
- acceptation des certificats auto-signés (déconseillé hors réseau de
  confiance) ;
- timeouts de test et de transcription.

L'audio est converti en WAV 16 kHz mono avant l'envoi. Un bouton de test permet
de vérifier la connexion.

### Accélération matérielle

Dans **Modèles**, le mode de calcul se règle sur **Auto**, **GPU** ou **CPU** :

- **Auto** utilise le GPU quand il est disponible, sinon le CPU ;
- **GPU** force l'exécution GPU (DirectML, WebGPU ou CoreML selon le système) ;
- **CPU** fonctionne sur tous les ordinateurs et permet de régler le nombre de
  threads.

Si aucun GPU compatible n'est détecté, l'application indique la raison et
continue en CPU.

## Les écrans

### Nouveau

Statut du moteur actif, sélection du moteur, enregistrement micro, import par
glisser-déposer, résultat de la transcription et copie dans le presse-papiers.
La copie automatique peut être désactivée dans les paramètres.

### Historique

Liste des transcriptions avec lecture de l'audio, texte original et texte
corrigé. Chaque entrée a un statut : **Original**, **À corriger** ou
**Corrigé**. Seules les entrées corrigées sont exportées par défaut.

### Modèles

Import, chargement et déchargement des modèles locaux, ajout et test des
serveurs, choix du mode GPU/CPU et réglage des threads.

### Paramètres

- raccourci global ;
- durée minimale d'enregistrement et taille maximale des fichiers ;
- découpage des longs audios et timeouts serveur ;
- langue ISO 639-3 et locuteur pour l'export MMS ;
- limite d'entrées chargées dans l'historique ;
- thème, comportement en arrière-plan, micro et mises à jour ;
- réouverture de la fenêtre « Nouveautés ».

## Export MMS

L'export crée un dossier daté :

```text
export_mms_2026-11-12_16-30-00/
├── dataset.tsv
├── README.md
└── audio/
    ├── <id>.wav
    └── ...
```

`dataset.tsv` contient les colonnes suivantes :

| Colonne | Contenu |
| --- | --- |
| `path` | chemin relatif du fichier audio |
| `text` | texte corrigé |
| `language` | code ISO 639-3 (`fmp` par défaut) |
| `speaker_id` | identifiant du locuteur (`speaker_001` par défaut) |
| `duration` | durée en secondes |

Le fichier `README.md` récapitule le nombre d'entrées, la durée totale, la
langue et le locuteur. L'option **Exporter uniquement les corrigées** est
activée par défaut ; la désactiver inclut aussi les transcriptions non
corrigées.

## Données et confidentialité

Les données de l'application sont stockées dans le dossier de données associé à
l'identifiant `com.dakindrago.to-nju` :

| Système | Chemin |
| --- | --- |
| Windows | `%APPDATA%\com.dakindrago.to-nju\` |
| Linux | `~/.local/share/com.dakindrago.to-nju/` |
| macOS | `~/Library/Application Support/com.dakindrago.to-nju/` |

On y trouve :

- `models/` : modèles ONNX importés ;
- `history/index.json` : métadonnées de l'historique ;
- `history/audio/` : fichiers audio des transcriptions.

Les réglages sont conservés dans le stockage local de la WebView. L'application
ne contient aucune télémétrie. En mode local, aucun audio ne quitte la machine.
En mode serveur, l'audio est envoyé au serveur configuré, et à lui seul.

## Développement

### Prérequis

- [Bun](https://bun.sh/) ;
- Rust stable ;
- les [dépendances Tauri](https://v2.tauri.app/start/prerequisites/) pour ton
  système ;
- sous Linux : `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`
  et `patchelf` ;
- sous Windows : WebView2 et les outils de build MSVC ;
- sous macOS : les outils en ligne de commande Xcode.

### Commandes

```bash
bun install            # dépendances frontend
bun run tauri dev      # application en mode développement
bun run check          # vérification TypeScript/Svelte
bun run build          # build du frontend seul
bun run tauri build    # installateur de la plateforme courante
bun run build:mac      # macOS : .app + .dmg + .pkg (macOS uniquement)
```

### Notes de build

- `src-tauri/build.rs` télécharge et met en cache ONNX Runtime ainsi que les
  execution providers natifs (DirectML sous Windows, WebGPU sous Linux, CoreML
  sous macOS). Le premier build nécessite donc un accès réseau.
- Sans `TAURI_SIGNING_PRIVATE_KEY` (et éventuellement
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), `tauri build` produit quand même les
  paquets, mais se termine en erreur faute de pouvoir signer l'updater.
- Le paquet macOS `.pkg` est fabriqué par `scripts/make-macos-pkg.sh` à partir
  du `.app` produit par Tauri. Il n'est pas signé.
- Les releases sont déclenchées par un tag `v*`. Le workflow GitHub Actions
  construit Windows, Linux et macOS, crée la release, publie les paquets et
  génère `latest.json` pour les mises à jour.

### Structure du dépôt

```text
src/                    interface SvelteKit
├── routes/             Nouveau, Historique, Modèles, Paramètres
└── lib/                moteurs, réglages, thème, changelog
src-tauri/
├── src/                local, server, history, ctc, options, fsops
├── build.rs            staging ONNX Runtime + execution providers
└── tauri.conf.json     configuration Tauri
scripts/
└── make-macos-pkg.sh   fabrication du .pkg macOS
.github/workflows/      CI et release
```

## Dépannage

**Le micro n'apparaît pas.** Vérifie les permissions micro du système,
débranche/rebranche le périphérique et utilise le bouton d'actualisation de la
liste des micros.

**Le GPU est indisponible.** Passe en mode **Auto** ou **CPU**. Sous Windows,
vérifie le pilote DirectX 12 et la présence de DirectML ; sous Linux, le support
WebGPU ; sous macOS, une version récente de macOS sur Apple Silicon.

**Le modèle est refusé.** Vérifie que le dossier contient bien `model.onnx`,
`vocab.json` et `meta.json`, et que le modèle a été exporté avec une version
compatible d'`export_onnx.py`.

**Un long audio échoue ou sature la mémoire.** Réduis la durée maximale par
morceau dans les paramètres et relance la transcription.

**La fenêtre se ferme au démarrage sous Linux.** Le renderer DMABUF de
WebKitGTK est désactivé automatiquement. Tu peux aussi définir
`WEBKIT_DISABLE_DMABUF_RENDERER=1` avant de lancer l'application.

**F5 et Ctrl+R ne rechargent pas la fenêtre.** C'est volontaire en version
installée : le rechargement de la WebView est bloqué.

**La mise à jour échoue.** Vérifie la connexion réseau et l'accès à
`https://github.com/DakinDrago/Nju/releases/latest/download/latest.json`, puis
relance l'application. Les mises à jour publiées sont signées.

## Contribuer

Les retours et les contributions sont bienvenus. Avant d'ouvrir une pull
request, lance `bun run check` et `bun run build`, et décris clairement le
changement. Pour un bug, précise la plateforme, la version et les étapes de
reproduction.

## Licence

Njū' est distribué sous licence **GNU General Public License v3.0**
(`GPL-3.0-only`). Voir [LICENSE](LICENSE).
