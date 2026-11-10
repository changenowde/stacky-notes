# Stacky Notes

Ein kleines Fenster, um schnell eine Note auf Substack zu posten.
Läuft unter Windows, macOS und Linux, ohne dass Python oder sonst etwas installiert sein muss.

> **Kein offizielles Produkt von Substack Inc.** Stacky Notes ist ein unabhängiges Projekt und steht in keiner Verbindung zu Substack.
> „Substack“ ist eine Marke von Substack Inc. Die App nutzt eine nicht offiziell dokumentierte Schnittstelle, die Substack jederzeit ändern kann.

## Download & Installation

Die fertigen Dateien gibt es unter [**Releases**](https://github.com/changenowde/stacky-notes/releases/latest).

**Windows**: `StackyNotes-windows-x64.exe` herunterladen und doppelklicken.
Erscheint „Der Computer wurde durch Windows geschützt“, auf *Weitere Informationen* und dann auf *Trotzdem ausführen* klicken.

**macOS** (Apple Silicon und Intel, ab macOS 11): `StackyNotes-macos-universal.dmg` öffnen und die App in „Programme“ ziehen.
Beim ersten Start blockiert macOS die App, weil sie nicht mit einem Apple-Entwicklerkonto signiert ist. So gibst du sie frei:
*Systemeinstellungen* → *Datenschutz & Sicherheit* → ganz nach unten scrollen → *Dennoch öffnen*. Das ist nur einmal nötig.

**Linux** (x64): `StackyNotes-linux-x64.tar.gz` entpacken und `./install.sh` ausführen.
Das legt die App für deinen Benutzer an, mit Eintrag im Anwendungsmenü. Root-Rechte sind nicht nötig.
Entfernen geht mit `./install.sh --uninstall`. Alternativ kannst du `./stacky-notes` auch direkt starten.

## Benutzung

1. Beim ersten Mal unter **Einstellungen → Login (Cookie einrichten)** dein `substack.sid`-Cookie einfügen (siehe unten).
2. Text schreiben und auf **Abschicken** klicken, oder **Strg+Enter** drücken (auf dem Mac auch **⌘+Enter**).
3. **Esc** oder **Abbrechen** schließt das Fenster.

### Wo finde ich das `substack.sid`-Cookie?

1. Im Browser bei substack.com einloggen.
2. Entwicklertools öffnen (F12, auf dem Mac ⌥⌘I).
3. Chrome/Edge: Reiter *Anwendung* (Application) → *Cookies* → `https://substack.com`.
   Firefox: Reiter *Web-Speicher* (Storage) → *Cookies*. Safari: *Web-Inspektor* → *Speicher*.
4. Den Wert von `substack.sid` kopieren und in der App einfügen.

Das Cookie ist praktisch dein Passwort, gib es also niemandem weiter.
Wenn Substack es irgendwann ungültig macht, meldet die App einen Fehler 401 oder 403. Dann einfach das neue Cookie eintragen.

### Wo werden die Einstellungen gespeichert?

| System  | Pfad |
|---------|------|
| Windows | `%APPDATA%\StackyNotes\config\config.json` |
| macOS   | `~/Library/Application Support/StackyNotes/config.json` |
| Linux   | `~/.config/stackynotes/config.json` |

Unter macOS und Linux kann nur dein eigener Benutzer die Datei lesen (Rechte 600).

---

## Für Entwickler

### Lokal starten

Du brauchst nur [Rust](https://rustup.rs). Dann:

```sh
cargo run            # Debug-Build mit Konsolenausgabe
cargo build --release
cargo test
```

Unter Linux müssen die üblichen Desktop-Bibliotheken vorhanden sein (X11 oder Wayland, Mesa), was auf jedem Desktop-System der Fall ist.

### Neues Release veröffentlichen

1. Version in `Cargo.toml` erhöhen (z. B. `version = "0.2.0"`) und committen.
2. Tag setzen und pushen:
   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```
3. GitHub Actions baut daraufhin alle drei Versionen und legt ein Release mit Installationshinweisen an. Das dauert etwa 10–15 Minuten.

Einen Testbuild ohne Release startest du im Reiter *Actions* → *Release* → *Run workflow*.
Die Dateien liegen dann unten im Workflow-Lauf unter *Artifacts*.

Da die Dateinamen immer gleich bleiben, kannst du feste Links zur jeweils neuesten Version setzen:

```
https://github.com/changenowde/stacky-notes/releases/latest/download/StackyNotes-windows-x64.exe
https://github.com/changenowde/stacky-notes/releases/latest/download/StackyNotes-macos-universal.dmg
https://github.com/changenowde/stacky-notes/releases/latest/download/StackyNotes-linux-x64.tar.gz
```

### Was der Workflow macht

| Plattform | Ergebnis | Besonderheiten |
|-----------|----------|----------------|
| Windows | eine einzelne `.exe` | Icon eingebettet, C-Laufzeitbibliothek fest eingebaut (läuft auch auf frischen Systemen) |
| macOS | `.dmg` mit Universal-App | ein Paket für Apple Silicon und Intel; Ad-hoc-signiert, damit „Dennoch öffnen“ funktioniert |
| Linux | `.tar.gz` mit Install-Skript | gegen glibc 2.28 gebaut, läuft also auch auf älteren Distributionen |

### Projektstruktur

```
src/main.rs                     die komplette App
build.rs                        bettet unter Windows das Icon in die .exe ein
assets/                         Icons (PNG, ICO, ICNS)
packaging/macos/Info.plist      Metadaten des macOS-App-Pakets
packaging/linux/                .desktop-Datei und Install-Skript
.github/workflows/release.yml   der Build- und Release-Workflow
.cargo/config.toml              Windows-Linkeroption
```
