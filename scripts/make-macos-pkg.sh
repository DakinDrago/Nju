#!/usr/bin/env bash
# Fabrique le paquet d'installation macOS (.pkg) à partir du .app produit
# par Tauri. À exécuter sur macOS, après `bun run tauri build`.
#
# Usage :
#   bash scripts/make-macos-pkg.sh [target-triple]
#
# Sans argument, utilise src-tauri/target/release/bundle/macos (build local).
# Avec un triplet (ex. aarch64-apple-darwin), utilise le dossier du build
# correspondant, comme le fait la CI.
#
# Le paquet n'est pas signé : Gatekeeper affichera un avertissement tant
# qu'aucun certificat « Developer ID Installer » n'est utilisé.

set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:-}"
BUNDLE_DIR="src-tauri/target${TARGET:+/$TARGET}/release/bundle"

APP_NAME="$(awk -F'"' '/"productName"/ { print $4; exit }' src-tauri/tauri.conf.json)"
if [ -z "$APP_NAME" ]; then
  echo "productName introuvable dans src-tauri/tauri.conf.json" >&2
  exit 1
fi

APP="$BUNDLE_DIR/macos/${APP_NAME}.app"
if [ ! -d "$APP" ]; then
  echo "Application .app introuvable : $APP" >&2
  echo "Lance d'abord : bun run tauri build${TARGET:+ --target $TARGET}" >&2
  exit 1
fi

VERSION="$(awk -F'"' '/^version = / { print $2; exit }' src-tauri/Cargo.toml)"
if [ -z "$VERSION" ]; then
  echo "Version introuvable dans src-tauri/Cargo.toml" >&2
  exit 1
fi

ARCH="${TARGET%%-*}"
if [ -z "$ARCH" ]; then
  case "$(uname -m)" in
    arm64) ARCH="aarch64" ;;
    *) ARCH="$(uname -m)" ;;
  esac
fi

OUT_DIR="$BUNDLE_DIR/pkg"
mkdir -p "$OUT_DIR"

PKG="$OUT_DIR/${APP_NAME}_${VERSION}_${ARCH}.pkg"
rm -f "$PKG"

# Installation dans /Applications, comme un installeur macOS classique.
productbuild --component "$APP" /Applications "$PKG"

echo "Paquet créé : $PKG"
