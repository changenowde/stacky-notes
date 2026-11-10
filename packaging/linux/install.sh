#!/bin/sh
# Installiert Stacky Notes für den aktuellen Benutzer (ohne root).
#   ./install.sh               installieren bzw. aktualisieren
#   ./install.sh --uninstall   wieder entfernen
set -e

SRC="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$HOME/.local/bin"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/256x256/apps"

if [ "$1" = "--uninstall" ]; then
    rm -f "$BIN_DIR/stacky-notes" \
          "$APPS_DIR/stacky-notes.desktop" \
          "$ICON_DIR/stacky-notes.png"
    echo "Stacky Notes wurde entfernt."
    exit 0
fi

mkdir -p "$BIN_DIR" "$APPS_DIR" "$ICON_DIR"
install -m 755 "$SRC/stacky-notes" "$BIN_DIR/stacky-notes"
install -m 644 "$SRC/stacky-notes.png" "$ICON_DIR/stacky-notes.png"
sed "s|^Exec=.*|Exec=$BIN_DIR/stacky-notes|" \
    "$SRC/stacky-notes.desktop" > "$APPS_DIR/stacky-notes.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
fi

echo "Fertig! Stacky Notes ist jetzt in deinem Anwendungsmenü."
