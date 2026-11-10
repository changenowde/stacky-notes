// Kein Konsolenfenster unter Windows (nur im Release-Build).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Key, KeyboardShortcut, Layout, Margin, Modifiers,
    RichText, Stroke, TextStyle,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Farbschema (wie in der Python-Version)
// ---------------------------------------------------------------------------
const BG_PRIMARY: Color32 = Color32::from_rgb(0x4b, 0x6c, 0x7d); // Blau (Haupthintergrund)
const BG_PRIMARY_ALT: Color32 = Color32::from_rgb(0x50, 0x6d, 0x7d); // Dunkelblau (Textfeld)
const BG_SECONDARY: Color32 = Color32::from_rgb(0x72, 0x84, 0x86); // Türkis (Abbrechen)
const BORDER_SAND: Color32 = Color32::from_rgb(0x9d, 0x85, 0x69); // Sand (Rahmen & Senden)
const TEXT_WHITE: Color32 = Color32::WHITE;
const INPUT_BG: Color32 = Color32::from_rgb(0x3f, 0x5c, 0x6b); // etwas dunkler, für Eingabefelder in Dialogen

/// Schriftgröße in egui-Punkten (entspricht optisch etwa 15pt in Tk).
const FONT_SIZE: f32 = 17.0;

const API_URL: &str = "https://substack.com/api/v1/comment/feed/";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)";
const APP_TITLE: &str = "Stacky Notes";

// ---------------------------------------------------------------------------
// Einstellungen
// ---------------------------------------------------------------------------

/// Wird im Einstellungsordner des Benutzers gespeichert:
/// - Windows: %APPDATA%\StackyNotes\config\config.json
/// - macOS:   ~/Library/Application Support/StackyNotes/config.json
/// - Linux:   ~/.config/stackynotes/config.json
#[derive(Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    cookie: String,
}

fn config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "StackyNotes")
        .map(|dirs| dirs.config_dir().join("config.json"))
}

impl Config {
    fn load() -> Self {
        config_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    fn save(&self) -> Result<(), String> {
        let path = config_path().ok_or("Kein Einstellungsordner gefunden.")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        write_private(&path, json.as_bytes()).map_err(|e| e.to_string())
    }
}

/// Schreibt die Datei so, dass nur der eigene Benutzer sie lesen kann
/// (das Cookie ist praktisch dein Passwort).
#[cfg(unix)]
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(data)
}

/// Unter Windows ist %APPDATA% ohnehin nur für den eigenen Benutzer lesbar.
#[cfg(not(unix))]
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    fs::write(path, data)
}

/// Akzeptiert sowohl nur den Wert als auch "substack.sid=Wert".
fn normalize_cookie(input: &str) -> String {
    let value = input.trim();
    let value = value.strip_prefix("substack.sid=").unwrap_or(value);
    value.trim_end_matches(';').trim().to_string()
}

// ---------------------------------------------------------------------------
// Senden an Substack
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum SendOutcome {
    Posted,
    ApiError { status: u16, body: String },
    Network(String),
}

fn default_agent() -> ureq::Agent {
    // Zertifikate des Betriebssystems verwenden (funktioniert auch hinter
    // Virenscannern oder Firmen-Proxys, die HTTPS-Verbindungen prüfen).
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();

    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into()
}

fn post_note(agent: &ureq::Agent, url: &str, cookie: &str, text: &str) -> SendOutcome {
    let result = agent
        .post(url)
        .header("Cookie", &format!("substack.sid={cookie}"))
        .header("User-Agent", USER_AGENT)
        .send_json(serde_json::json!({ "body": text }));

    match result {
        Ok(mut response) => {
            let status = response.status().as_u16();
            if status == 200 || status == 201 {
                SendOutcome::Posted
            } else {
                let body = response.body_mut().read_to_string().unwrap_or_default();
                SendOutcome::ApiError {
                    status,
                    body: shorten(&body, 600),
                }
            }
        }
        Err(err) => SendOutcome::Network(err.to_string()),
    }
}

fn shorten(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max_chars).collect();
        format!("{cut} …")
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

enum Dialog {
    None,
    Login {
        input: String,
        hint: Option<String>,
        show_plain: bool,
        focus: bool,
    },
    Message {
        title: String,
        text: String,
        quit_after: bool,
    },
}

struct QuickNoteApp {
    config: Config,
    note: String,
    dialog: Dialog,
    sending: Option<Receiver<SendOutcome>>,
    focus_note: bool,
}

impl QuickNoteApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        Self {
            config: Config::load(),
            note: String::new(),
            dialog: Dialog::None,
            sending: None,
            focus_note: true,
        }
    }

    fn open_login(&mut self, hint: Option<&str>) {
        self.dialog = Dialog::Login {
            input: self.config.cookie.clone(),
            hint: hint.map(str::to_string),
            show_plain: false,
            focus: true,
        };
    }

    fn message(&mut self, title: &str, text: impl Into<String>) {
        self.dialog = Dialog::Message {
            title: title.to_string(),
            text: text.into(),
            quit_after: false,
        };
    }

    fn send(&mut self, ctx: &egui::Context) {
        if self.sending.is_some() {
            return;
        }
        let text = self.note.trim().to_string();

        if self.config.cookie.is_empty() {
            self.open_login(Some("Bitte richte zuerst den Login ein."));
            return;
        }
        if text.is_empty() {
            self.message("Fehler", "Deine Note ist komplett leer.");
            return;
        }

        let cookie = self.config.cookie.clone();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let outcome = post_note(&default_agent(), API_URL, &cookie, &text);
            let _ = tx.send(outcome);
            ctx.request_repaint();
        });
        self.sending = Some(rx);
    }

    fn poll_sending(&mut self) {
        let Some(rx) = &self.sending else { return };
        let outcome = match rx.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                SendOutcome::Network("Der Sendevorgang wurde unerwartet beendet.".into())
            }
        };
        self.sending = None;

        match outcome {
            SendOutcome::Posted => {
                self.dialog = Dialog::Message {
                    title: "Erfolg".into(),
                    text: "Note wurde erfolgreich auf Substack gepostet!".into(),
                    quit_after: true,
                };
            }
            SendOutcome::ApiError { status, body } => {
                let mut text = format!("Substack API Fehler ({status})");
                if status == 401 || status == 403 {
                    text.push_str(
                        "\n\nDein Cookie ist vermutlich abgelaufen. \
                         Richte den Login unter Einstellungen neu ein.",
                    );
                }
                if !body.is_empty() {
                    text.push_str("\n\n");
                    text.push_str(&body);
                }
                self.message("Fehler", text);
            }
            SendOutcome::Network(err) => {
                self.message("Fehler", format!("Verbindung fehlgeschlagen:\n{err}"));
            }
        }
    }

    fn quit(ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // --- Dialoge -----------------------------------------------------------

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let dialog = std::mem::replace(&mut self.dialog, Dialog::None);
        self.dialog = match dialog {
            Dialog::None => Dialog::None,
            Dialog::Login {
                mut input,
                hint,
                mut show_plain,
                focus,
            } => {
                let mut save = false;
                let mut cancel = false;

                let modal = egui::Modal::new(egui::Id::unique("login"))
                    .frame(dialog_frame())
                    .show(ctx, |ui| {
                        ui.set_width(480.0);
                        ui.label(RichText::new("Substack Login").strong());
                        ui.add_space(6.0);
                        if let Some(hint) = &hint {
                            ui.label(RichText::new(hint).color(BORDER_SAND_LIGHT));
                        }
                        ui.label("Füge deinen 'substack.sid' Cookie hier ein:");
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut input)
                                .password(!show_plain)
                                .desired_width(f32::INFINITY)
                                .margin(Margin::symmetric(8, 6)),
                        );
                        if focus {
                            field.request_focus();
                        }
                        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            save = true;
                        }
                        ui.checkbox(&mut show_plain, "Cookie anzeigen");
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.add(colored_button("Abbrechen", BG_SECONDARY)).clicked() {
                                cancel = true;
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(colored_button("Speichern", BORDER_SAND)).clicked() {
                                    save = true;
                                }
                            });
                        });
                    });
                if modal.should_close() {
                    cancel = true;
                }

                if save {
                    let cookie = normalize_cookie(&input);
                    if cookie.is_empty() {
                        Dialog::Login {
                            input,
                            hint: Some("Das Feld ist leer.".into()),
                            show_plain,
                            focus: true,
                        }
                    } else {
                        self.config.cookie = cookie;
                        self.focus_note = true;
                        match self.config.save() {
                            Ok(()) => Dialog::Message {
                                title: "Erfolg".into(),
                                text: "Cookie erfolgreich gespeichert!".into(),
                                quit_after: false,
                            },
                            Err(err) => Dialog::Message {
                                title: "Fehler".into(),
                                text: format!(
                                    "Cookie konnte nicht gespeichert werden:\n{err}\n\n\
                                     Es gilt trotzdem bis zum Schließen der App."
                                ),
                                quit_after: false,
                            },
                        }
                    }
                } else if cancel {
                    self.focus_note = true;
                    Dialog::None
                } else {
                    Dialog::Login {
                        input,
                        hint,
                        show_plain,
                        focus: false,
                    }
                }
            }
            Dialog::Message {
                title,
                text,
                quit_after,
            } => {
                let mut close = false;
                let modal = egui::Modal::new(egui::Id::unique("message"))
                    .frame(dialog_frame())
                    .show(ctx, |ui| {
                        ui.set_max_width(480.0);
                        ui.label(RichText::new(&title).strong());
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| ui.label(&text));
                        ui.add_space(10.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(colored_button("OK", BORDER_SAND)).clicked() {
                                close = true;
                            }
                        });
                    });
                if modal.should_close() || ctx.input(|i| i.key_pressed(Key::Enter)) {
                    close = true;
                }

                if close {
                    if quit_after {
                        Self::quit(ctx);
                    }
                    self.focus_note = true;
                    Dialog::None
                } else {
                    Dialog::Message {
                        title,
                        text,
                        quit_after,
                    }
                }
            }
        };
    }
}

impl eframe::App for QuickNoteApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_sending();

        let dialog_open = !matches!(self.dialog, Dialog::None);
        let is_sending = self.sending.is_some();

        // Tastenkürzel. Vor den Widgets abfragen, damit Strg+Enter
        // keinen Zeilenumbruch ins Textfeld schreibt.
        if !dialog_open {
            let send_pressed = ctx.input_mut(|i| {
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter))
                    || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::Enter))
            });
            if send_pressed {
                self.send(&ctx);
            }
            if ctx.input(|i| i.key_pressed(Key::Escape)) && !egui::Popup::is_any_open(&ctx) {
                Self::quit(&ctx);
            }
        }

        // Menüleiste
        egui::Panel::top("menu")
            .frame(
                egui::Frame::new()
                    .fill(BG_PRIMARY_ALT)
                    .inner_margin(Margin::symmetric(8, 4)),
            )
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("Einstellungen", |ui| {
                        if ui.button("Login (Cookie einrichten)").clicked() {
                            self.open_login(None);
                        }
                    });
                });
            });

        // Buttons unten
        egui::Panel::bottom("buttons")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(BG_PRIMARY).inner_margin(Margin::same(20)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.add(colored_button("Abbrechen", BG_SECONDARY)).clicked() {
                        Self::quit(&ctx);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = if is_sending { "Sende …" } else { "Abschicken" };
                        let send = ui.add_enabled(!is_sending, colored_button(label, BORDER_SAND));
                        if send.clicked() {
                            self.send(&ctx);
                        }
                    });
                });
            });

        // Überschrift und Textfeld
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG_PRIMARY).inner_margin(Margin {
                left: 20,
                right: 20,
                top: 12,
                bottom: 5,
            }))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| ui.label("Was gibt's Neues?"));
                ui.add_space(12.0);

                egui::Frame::new()
                    .fill(BG_PRIMARY_ALT)
                    .stroke(Stroke::new(2.0, BORDER_SAND))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let editor = ui.add(
                                    egui::TextEdit::multiline(&mut self.note)
                                        .frame(egui::Frame::NONE.inner_margin(Margin::same(10)))
                                        .desired_width(f32::INFINITY)
                                        .min_size(ui.available_size())
                                        .interactive(!is_sending),
                                );
                                if self.focus_note && !dialog_open {
                                    editor.request_focus();
                                    self.focus_note = false;
                                }
                            });
                    });
            });

        self.show_dialog(&ctx);
    }
}

// ---------------------------------------------------------------------------
// Aussehen
// ---------------------------------------------------------------------------

const BORDER_SAND_LIGHT: Color32 = Color32::from_rgb(0xd9, 0xc3, 0xa5);

fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        for (text_style, font) in style.text_styles.iter_mut() {
            let size = if *text_style == TextStyle::Small {
                FONT_SIZE * 0.8
            } else {
                FONT_SIZE
            };
            *font = FontId::monospace(size);
        }

        style.spacing.button_padding = egui::vec2(15.0, 5.0);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.menu_margin = Margin::same(6);

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT_WHITE);
        v.panel_fill = BG_PRIMARY;
        v.window_fill = BG_PRIMARY;
        v.window_stroke = Stroke::new(2.0, BORDER_SAND);
        v.extreme_bg_color = INPUT_BG;
        v.text_edit_bg_color = Some(INPUT_BG);
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(0x9d, 0x85, 0x69, 170);
        v.selection.stroke = Stroke::new(1.5, BORDER_SAND_LIGHT);
        v.text_cursor.stroke = Stroke::new(2.0, TEXT_WHITE);

        // Menüs, Checkbox & Co.
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SAND);
        v.widgets.inactive.weak_bg_fill = BG_PRIMARY_ALT;
        v.widgets.inactive.bg_fill = INPUT_BG;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SAND);
        v.widgets.hovered.weak_bg_fill = BG_SECONDARY;
        v.widgets.hovered.bg_fill = BG_SECONDARY;
        v.widgets.hovered.bg_stroke = Stroke::new(1.5, BORDER_SAND_LIGHT);
        v.widgets.active.weak_bg_fill = BG_SECONDARY;
        v.widgets.active.bg_fill = BG_SECONDARY;
        v.widgets.open.weak_bg_fill = BG_SECONDARY;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.fg_stroke.color = TEXT_WHITE;
        }
    });
}

fn colored_button(text: &str, fill: Color32) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(TEXT_WHITE))
        .fill(fill)
        .corner_radius(CornerRadius::ZERO)
}

fn dialog_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(BG_PRIMARY)
        .stroke(Stroke::new(2.0, BORDER_SAND))
        .inner_margin(Margin::same(20))
}

// ---------------------------------------------------------------------------

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_app_id("stacky-notes")
        .with_inner_size([600.0, 450.0])
        .with_min_inner_size([420.0, 320.0]);

    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")) {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(QuickNoteApp::new(cc)))),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_is_normalized() {
        assert_eq!(normalize_cookie("  abc123  "), "abc123");
        assert_eq!(normalize_cookie("substack.sid=abc123;"), "abc123");
        assert_eq!(normalize_cookie(""), "");
    }

    #[test]
    fn long_bodies_are_shortened() {
        assert_eq!(shorten("kurz", 10), "kurz");
        assert_eq!(shorten("äöüäöü", 3), "äöü …");
    }

    /// Prüft den echten HTTP-Aufruf gegen einen lokalen Testserver.
    /// Erwartet die URL in SQN_TEST_URL, sonst wird der Test übersprungen.
    #[test]
    fn posts_note_to_test_server() {
        let Some(url) = std::env::var("SQN_TEST_URL").ok().filter(|u| !u.is_empty()) else {
            eprintln!("SQN_TEST_URL nicht gesetzt, übersprungen");
            return;
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into();

        assert_eq!(
            post_note(&agent, &format!("{url}/ok"), "geheim", "Hallo Welt"),
            SendOutcome::Posted
        );
        match post_note(&agent, &format!("{url}/forbidden"), "geheim", "x") {
            SendOutcome::ApiError { status, body } => {
                assert_eq!(status, 403);
                assert!(body.contains("nope"));
            }
            other => panic!("unerwartet: {other:?}"),
        }
    }
}
