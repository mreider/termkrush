//! The egui/eframe desktop front-end — the mouse-first replacement for the
//! TUI (see the 2026-06-08 GUI pivot in `.am/inception.md`). The engine
//! (`termkrush-core`) is unchanged; this is all view + input.
//!
//! Mouse model: drag a track onto a pad to load it; drag a track into a folder
//! to move it; double-click to rename; select + a delete button to delete;
//! click a track's ▶ to preview. No modal dialogs — inline fields and buttons.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

use eframe::egui;
use termkrush_core::audio::{
    decode_file, detect_bpm, probe_playable, write_wav, AudioOutput, DecodedAudio,
};
use termkrush_core::config::Config;
use termkrush_core::library::Crate;
use termkrush_core::mix::{Mixer, PadKind, PADS};

// CRT identity, modernized: near-black ground, green text, amber accents.
const GREEN: egui::Color32 = egui::Color32::from_rgb(0x3a, 0xf0, 0x6a);
const AMBER: egui::Color32 = egui::Color32::from_rgb(0xff, 0xb0, 0x14);
const RED: egui::Color32 = egui::Color32::from_rgb(0xff, 0x52, 0x52);
const GROUND: egui::Color32 = egui::Color32::from_rgb(0x0a, 0x0d, 0x0a);
const PANEL: egui::Color32 = egui::Color32::from_rgb(0x10, 0x14, 0x10);

/// Launch the desktop app. Blocks until the window closes.
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TermKrush")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "TermKrush",
        options,
        Box::new(|cc| Ok(Box::new(TermKrushApp::new(cc)))),
    )
}

/// A track being dragged out of the library (drop onto a pad or a folder).
#[derive(Clone)]
struct DragTrack(PathBuf);

/// Where a background decode is headed when it lands.
#[derive(Clone, Copy)]
enum Target {
    Pad(usize),
    Preview,
    /// Arm the scratch platter.
    Jog,
}

/// A finished background decode on its way to the audio engine.
struct LoadDone {
    target: Target,
    audio: DecodedAudio,
    bpm: Option<f32>,
    path: PathBuf,
}

/// A user intent gathered while drawing, applied after the frame so the draw
/// pass can borrow app state immutably.
enum Act {
    Select(PathBuf),
    EnterFolder(PathBuf),
    Preview(PathBuf),
    StartRename(PathBuf),
    CommitRename,
    CancelRename,
    Delete(PathBuf),
    MoveTo { track: PathBuf, folder: PathBuf },
    LoadToPad { pad: usize, path: PathBuf },
    StartNewFolder,
    CommitNewFolder,
    CancelNewFolder,
    PlayPad(usize),
    SetKind(usize, PadKind),
    SetGain(usize, f32),
    SetActive(usize, bool),
    ClearPad(usize),
    ExportPad(usize),
    EditClip(usize),
    CloseClip,
    SetTrimIn(usize, usize),
    SetTrimOut(usize, usize),
    AuditionSel(usize),
}

/// The whole app: the engine, the audio sink, and the browsed library.
pub struct TermKrushApp {
    mixer: Mixer,
    crate_lib: Crate,
    /// Source path loaded on each pad (for the cell's track name).
    pad_source: [Option<PathBuf>; PADS],
    /// Per-pad "decoding in the background" flag.
    loading: [bool; PADS],
    /// The selected library track (delete / preview act on it).
    lib_sel: Option<PathBuf>,
    /// Inline rename in progress: `(target, buffer)`.
    renaming: Option<(PathBuf, String)>,
    /// Inline new-folder name being typed.
    new_folder: Option<String>,
    /// Source loaded on the scratch platter (for its label).
    jog_source: Option<PathBuf>,
    /// The pad whose clip is open in the editor (central-panel mode).
    clip_edit: Option<usize>,

    producer: Option<rtrb::Producer<f32>>,
    _audio: Option<AudioOutput>,
    out_channels: usize,
    target_rate: u32,
    scratch: Vec<f32>,
    load_tx: Sender<LoadDone>,
    load_rx: Receiver<LoadDone>,

    /// Background playability probe: which tracks decode (red if not).
    playable: HashMap<PathBuf, bool>,
    probed_dir: Option<PathBuf>,
    probe_tx: Sender<(PathBuf, bool)>,
    probe_rx: Receiver<(PathBuf, bool)>,
}

impl TermKrushApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_crt_theme(&cc.egui_ctx);

        let cfg = Config::load();
        let crate_lib = Crate::scan(&cfg.crate_root);
        tracing::info!(root = %cfg.crate_root.display(), tracks = crate_lib.len(), "crate scanned (gui)");

        let (audio, producer, channels, rate) = match AudioOutput::start(1 << 13) {
            Ok((out, prod)) => {
                let (ch, r) = (out.channels as usize, out.sample_rate);
                (Some(out), Some(prod), ch, r)
            }
            Err(e) => {
                tracing::warn!(error = %e, "audio output unavailable; running without sound");
                (None, None, 2, 44_100)
            }
        };

        let mut mixer = Mixer::new();
        mixer.set_sample_rate(rate);
        let (load_tx, load_rx) = channel();
        let (probe_tx, probe_rx) = channel();

        Self {
            mixer,
            crate_lib,
            pad_source: Default::default(),
            loading: [false; PADS],
            lib_sel: None,
            renaming: None,
            new_folder: None,
            jog_source: None,
            clip_edit: None,
            producer,
            _audio: audio,
            out_channels: channels.max(1),
            target_rate: rate,
            scratch: Vec::new(),
            load_tx,
            load_rx,
            playable: HashMap::new(),
            probed_dir: None,
            probe_tx,
            probe_rx,
        }
    }

    /// Kick off a background playability probe for the current folder's tracks.
    fn spawn_probe(&mut self) {
        let dir = self.crate_lib.cwd().to_path_buf();
        let paths: Vec<PathBuf> = self
            .crate_lib
            .entries()
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        self.probed_dir = Some(dir);
        self.playable.clear();
        let tx = self.probe_tx.clone();
        std::thread::spawn(move || {
            for p in paths {
                let ok = probe_playable(&p);
                let _ = tx.send((p, ok));
            }
        });
    }

    /// Top up the audio ring with freshly mixed frames (never blocks).
    fn pump_audio(&mut self) {
        let Some(p) = self.producer.as_mut() else {
            return;
        };
        let ch = self.out_channels;
        let frames = p.slots() / ch;
        if frames == 0 {
            return;
        }
        self.scratch.resize(frames * 2, 0.0);
        self.mixer.fill_mix(&mut self.scratch);
        for f in 0..frames {
            let (l, r) = (self.scratch[f * 2], self.scratch[f * 2 + 1]);
            for c in 0..ch {
                let s = match c {
                    0 => l,
                    1 => r,
                    _ => 0.0,
                };
                let _ = p.push(s);
            }
        }
    }

    /// Decode `path` off-thread and route the result to `target`.
    fn spawn_load(&self, target: Target, path: PathBuf) {
        let tx = self.load_tx.clone();
        let rate = self.target_rate;
        std::thread::spawn(move || match decode_file(&path, rate) {
            Ok(audio) => {
                let bpm = matches!(target, Target::Pad(_))
                    .then(|| detect_bpm(&audio.samples, audio.channels, audio.sample_rate))
                    .flatten();
                let _ = tx.send(LoadDone {
                    target,
                    audio,
                    bpm,
                    path,
                });
            }
            Err(e) => tracing::error!(error = %e, path = %path.display(), "decode failed"),
        });
    }

    /// Drain finished decodes into the engine.
    fn drain_loads(&mut self) {
        while let Ok(done) = self.load_rx.try_recv() {
            match done.target {
                Target::Pad(i) => {
                    self.mixer.assign_pad(i, done.audio.samples);
                    if let Some(b) = done.bpm {
                        self.mixer.set_pad_bpm(i, Some(b));
                        // Auto-BPM: the first track to carry a tempo sets the master.
                        if self.mixer.master_bpm().is_none() {
                            self.mixer.set_master_bpm(Some(b));
                        }
                    }
                    self.pad_source[i] = Some(done.path);
                    self.loading[i] = false;
                }
                Target::Preview => self.mixer.preview(done.audio.samples),
                Target::Jog => self.mixer.set_jog_source(done.audio.samples),
            }
        }
    }

    fn apply(&mut self, act: Act) {
        match act {
            Act::Select(p) => self.lib_sel = Some(p),
            Act::EnterFolder(p) => {
                self.crate_lib.enter(&p);
                self.lib_sel = None;
            }
            Act::Preview(p) => {
                if self.mixer.is_previewing() {
                    self.mixer.stop_preview();
                } else {
                    self.spawn_load(Target::Preview, p);
                }
            }
            Act::StartRename(p) => {
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                self.renaming = Some((p, name));
            }
            Act::CommitRename => {
                if let Some((p, buf)) = self.renaming.take() {
                    let buf = buf.trim();
                    if !buf.is_empty() {
                        let _ = self.crate_lib.rename(&p, buf);
                    }
                }
            }
            Act::CancelRename => self.renaming = None,
            Act::Delete(p) => {
                let _ = self.crate_lib.delete(&p);
                if self.lib_sel.as_deref() == Some(p.as_path()) {
                    self.lib_sel = None;
                }
            }
            Act::MoveTo { track, folder } => {
                let _ = self.crate_lib.move_into(&track, &folder);
            }
            Act::LoadToPad { pad, path } => {
                self.loading[pad] = true;
                self.spawn_load(Target::Pad(pad), path);
            }
            Act::StartNewFolder => self.new_folder = Some(String::new()),
            Act::CommitNewFolder => {
                if let Some(name) = self.new_folder.take() {
                    let name = name.trim();
                    if !name.is_empty() {
                        let _ = self.crate_lib.make_folder(name);
                    }
                }
            }
            Act::CancelNewFolder => self.new_folder = None,
            Act::PlayPad(i) => {
                if self.mixer.pad_is_sounding(i) {
                    self.mixer.stop_pad(i);
                } else {
                    self.mixer.trigger_pad(i);
                }
            }
            Act::SetKind(i, k) => self.mixer.set_pad_kind(i, k),
            Act::SetGain(i, v) => self.mixer.set_pad_gain(i, v),
            Act::SetActive(i, on) => self.mixer.set_pad_active(i, on, true),
            Act::ClearPad(i) => {
                self.mixer.unload_pad(i);
                self.pad_source[i] = None;
            }
            Act::ExportPad(i) => self.export_pad(i),
            Act::EditClip(i) => {
                self.mixer.stop_pad(i); // silence live playback before editing
                self.clip_edit = Some(i);
            }
            Act::CloseClip => {
                if let Some(i) = self.clip_edit.take() {
                    self.mixer.stop_pad(i);
                }
            }
            Act::SetTrimIn(i, f) => self.mixer.set_pad_trim_in(i, f),
            Act::SetTrimOut(i, f) => self.mixer.set_pad_trim_out(i, f),
            Act::AuditionSel(i) => {
                if self.mixer.pad_is_sounding(i) {
                    self.mixer.stop_pad(i);
                } else {
                    let (inp, out) = self.mixer.pad_trim(i);
                    self.mixer.audition_region(i, inp, out);
                }
            }
        }
    }

    /// The scratch platter (bottom panel): drop a sound, then drag it (or use
    /// ←/→) to whip/wiki. Drives the engine's jog voice directly.
    fn draw_scratch_panel(&mut self, ctx: &egui::Context) {
        // Arrow-key jog: winit reports real key-up, so a held arrow sustains
        // and release stops — no terminal key-up hack needed.
        let dt = ctx.input(|i| i.stable_dt).max(1e-3) as f64;
        let key_vel: f32 = ctx.input(|i| {
            if i.key_down(egui::Key::ArrowRight) {
                1.5 // wiki (forward)
            } else if i.key_down(egui::Key::ArrowLeft) {
                -1.5 // whip (backward)
            } else {
                0.0
            }
        });

        egui::TopBottomPanel::bottom("scratch")
            .exact_height(140.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("SCRATCH").color(AMBER).strong());
                    let name = self
                        .jog_source
                        .as_ref()
                        .and_then(|p| p.file_stem())
                        .and_then(|s| s.to_str())
                        .unwrap_or("drag a sound here");
                    ui.label(egui::RichText::new(name).color(GREEN));
                    if self.mixer.has_jog() && ui.small_button("clear").clicked() {
                        self.mixer.clear_jog();
                        self.jog_source = None;
                    }
                });
                ui.label(
                    egui::RichText::new("drag the platter ↔ to scratch · or hold ← whip / → wiki")
                        .weak(),
                );
                ui.add_space(4.0);

                // The platter strip: a drop target for the sound + a drag
                // surface that jogs it. A playhead line shows the position.
                let width = ui.available_width();
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(width, 48.0), egui::Sense::click_and_drag());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, GROUND);
                painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, GREEN.gamma_multiply(0.5)));
                let len = self.mixer.jog_len();
                if len > 0 {
                    if let Some(pos) = self.mixer.jog_position() {
                        let frac = (pos / len as f64) as f32;
                        let x = rect.left() + frac * rect.width();
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            egui::Stroke::new(2.0, AMBER),
                        );
                    }
                }

                // Mouse drag → velocity (frames the drag covers, per output
                // frame). Dragging right spins forward, left backward.
                let vel = if resp.dragged() && len > 0 {
                    let dx = resp.drag_delta().x as f64;
                    (dx * len as f64 / (rect.width() as f64 * self.target_rate as f64 * dt)) as f32
                } else {
                    key_vel
                };
                self.mixer.set_jog_velocity(vel);

                // Drop a track onto the platter to arm it.
                if let Some(p) = resp.dnd_release_payload::<DragTrack>() {
                    self.jog_source = Some(p.0.clone());
                    self.spawn_load(Target::Jog, p.0.clone());
                }
            });
    }

    /// Write pad `i`'s trimmed clip to the current library folder as a WAV.
    fn export_pad(&mut self, i: usize) {
        let region = self.mixer.pad_clip_region(i);
        if region.is_empty() {
            return;
        }
        let stem = self.pad_source[i]
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("pad")
            .to_string();
        let dir = self.crate_lib.cwd().to_path_buf();
        let mut n = 1;
        let path = loop {
            let p = dir.join(format!("{stem}-edit{n}.wav"));
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        if let Err(e) = write_wav(&path, &region, self.mixer.sample_rate(), 2) {
            tracing::error!(error = %e, "export failed");
            return;
        }
        self.crate_lib.refresh();
        self.probed_dir = None; // re-probe so the new file gets checked
    }
}

impl eframe::App for TermKrushApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_audio();
        self.drain_loads();
        ctx.request_repaint(); // keep the audio ring fed in real time

        // Probe the current folder for unplayable files when it changes.
        while let Ok((p, ok)) = self.probe_rx.try_recv() {
            self.playable.insert(p, ok);
        }
        if self.probed_dir.as_deref() != Some(self.crate_lib.cwd()) {
            self.spawn_probe();
        }

        // The scratch platter (bottom) draws before the central pad grid so the
        // grid fills the space above it. It borrows &mut self directly.
        self.draw_scratch_panel(ctx);

        let mut acts: Vec<Act> = Vec::new();
        {
            let TermKrushApp {
                mixer,
                crate_lib,
                pad_source,
                loading,
                lib_sel,
                renaming,
                new_folder,
                playable,
                clip_edit,
                ..
            } = self;
            draw_timeline_strip(ctx, mixer);
            draw_library(
                ctx, crate_lib, lib_sel, renaming, new_folder, playable, &mut acts,
            );
            // Central panel: the clip editor when one is open, else the pads.
            if let Some(i) = *clip_edit {
                draw_clip_editor(ctx, mixer, pad_source, i, &mut acts);
            } else {
                draw_pad_grid(ctx, mixer, pad_source, loading, &mut acts);
            }
        }
        for a in acts {
            self.apply(a);
        }
    }
}

/// Apply the CRT amber/green look and a monospace face everywhere.
fn apply_crt_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(GREEN);
    v.panel_fill = GROUND;
    v.window_fill = PANEL;
    v.extreme_bg_color = GROUND;
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.inactive.bg_fill = PANEL;
    v.selection.bg_fill = AMBER.gamma_multiply(0.35);
    v.selection.stroke = egui::Stroke::new(1.0, AMBER);
    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    for font in style.text_styles.values_mut() {
        font.family = egui::FontFamily::Monospace;
    }
    ctx.set_style(style);
}

fn draw_timeline_strip(ctx: &egui::Context, mixer: &Mixer) {
    egui::TopBottomPanel::top("timeline")
        .exact_height(72.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("TermKrush").color(AMBER).strong());
                ui.add_space(16.0);
                let bpm = mixer
                    .master_bpm()
                    .map(|b| format!("♩ {b:.0} BPM"))
                    .unwrap_or_else(|| "♩ -- BPM".into());
                ui.label(egui::RichText::new(bpm).color(GREEN));
                ui.label(
                    egui::RichText::new(format!("· master {:.0}%", mixer.master_gain() * 100.0))
                        .weak(),
                );
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("TIMELINE  —  drag clips here (coming next)")
                    .color(AMBER)
                    .weak(),
            );
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_library(
    ctx: &egui::Context,
    lib: &Crate,
    sel: &Option<PathBuf>,
    renaming: &mut Option<(PathBuf, String)>,
    new_folder: &mut Option<String>,
    playable: &HashMap<PathBuf, bool>,
    acts: &mut Vec<Act>,
) {
    egui::SidePanel::left("library")
        .resizable(true)
        .default_width(260.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("LIBRARY").color(AMBER).strong());
                if ui.small_button("＋ folder").clicked() {
                    acts.push(Act::StartNewFolder);
                }
                let has_sel = sel.is_some();
                if ui
                    .add_enabled(has_sel, egui::Button::new("▶"))
                    .on_hover_text("preview")
                    .clicked()
                {
                    if let Some(p) = sel {
                        acts.push(Act::Preview(p.clone()));
                    }
                }
                if ui
                    .add_enabled(has_sel, egui::Button::new("🗑"))
                    .on_hover_text("delete selected")
                    .clicked()
                {
                    if let Some(p) = sel {
                        acts.push(Act::Delete(p.clone()));
                    }
                }
            });
            ui.separator();

            // Inline new-folder field.
            if let Some(buf) = new_folder.as_mut() {
                let resp = ui.add(
                    egui::TextEdit::singleline(buf)
                        .hint_text("new folder name")
                        .desired_width(f32::INFINITY),
                );
                if !resp.has_focus() {
                    resp.request_focus();
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    acts.push(Act::CommitNewFolder);
                } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    acts.push(Act::CancelNewFolder);
                }
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Up a level.
                if lib.cwd() != lib.root() {
                    if let Some(parent) = lib.cwd().parent() {
                        if ui.button("⬆ ..").clicked() {
                            acts.push(Act::EnterFolder(parent.to_path_buf()));
                        }
                    }
                }
                for e in lib.entries() {
                    if e.name == ".." {
                        continue; // handled by the explicit up button
                    }
                    if e.is_dir {
                        draw_folder_row(ui, &e.name, &e.path, acts);
                    } else {
                        let bad = playable.get(&e.path) == Some(&false);
                        draw_track_row(ui, e, sel, renaming, bad, acts);
                    }
                }
                if lib.is_empty() {
                    ui.label(egui::RichText::new("(empty — set crate_root)").weak());
                }
            });
        });
}

/// A folder row: click to open, a drop target to move a track in.
fn draw_folder_row(ui: &mut egui::Ui, name: &str, path: &Path, acts: &mut Vec<Act>) {
    let frame = egui::Frame::none().inner_margin(egui::Margin::symmetric(4.0, 2.0));
    let (inner, payload) = ui.dnd_drop_zone::<DragTrack, _>(frame, |ui| {
        ui.label(egui::RichText::new(format!("📁 {name}")).color(AMBER))
    });
    if inner.inner.clicked() || inner.response.clicked() {
        acts.push(Act::EnterFolder(path.to_path_buf()));
    }
    if let Some(p) = payload {
        acts.push(Act::MoveTo {
            track: p.0.clone(),
            folder: path.to_path_buf(),
        });
    }
}

/// A track row: drag source (load/move), click to select, double-click rename.
fn draw_track_row(
    ui: &mut egui::Ui,
    e: &termkrush_core::library::CrateEntry,
    sel: &Option<PathBuf>,
    renaming: &mut Option<(PathBuf, String)>,
    bad: bool,
    acts: &mut Vec<Act>,
) {
    // Inline rename for this row.
    if let Some((p, buf)) = renaming.as_mut() {
        if p == &e.path {
            let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(f32::INFINITY));
            if !resp.has_focus() {
                resp.request_focus();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                acts.push(Act::CommitRename);
            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                acts.push(Act::CancelRename);
            }
            return;
        }
    }

    let selected = sel.as_deref() == Some(e.path.as_path());
    let id = egui::Id::new(("track", &e.path));
    let resp = ui
        .dnd_drag_source(id, DragTrack(e.path.clone()), |ui| {
            let mut text = egui::RichText::new(&e.name);
            if bad {
                text = text.color(RED); // unplayable / failed to decode
            } else if selected {
                text = text.color(AMBER).strong();
            }
            let label = ui.label(text);
            if bad {
                label.on_hover_text("unplayable — won't decode");
            }
        })
        .response;
    if resp.clicked() {
        acts.push(Act::Select(e.path.clone()));
    }
    if resp.double_clicked() {
        acts.push(Act::StartRename(e.path.clone()));
    }
}

fn draw_pad_grid(
    ctx: &egui::Context,
    mixer: &Mixer,
    pad_source: &[Option<PathBuf>; PADS],
    loading: &[bool; PADS],
    acts: &mut Vec<Act>,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("PADS  —  drag a track onto a pad to load")
                .color(AMBER)
                .strong(),
        );
        ui.add_space(6.0);
        let cols = 4;
        let spacing = ui.spacing().item_spacing;
        let cell_w =
            ((ui.available_width() - spacing.x * (cols as f32 - 1.0)) / cols as f32).floor();
        egui::Grid::new("pads")
            .num_columns(cols)
            .spacing([spacing.x, spacing.y])
            .show(ui, |ui| {
                for i in 0..PADS {
                    draw_pad_cell(ui, mixer, pad_source, loading, i, cell_w, acts);
                    if (i + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
    });
}

/// The clip editor: a full-clip waveform with draggable in/out handles. With a
/// mouse the handles are precise enough that no zoom window is needed.
fn draw_clip_editor(
    ctx: &egui::Context,
    mixer: &Mixer,
    pad_source: &[Option<PathBuf>; PADS],
    i: usize,
    acts: &mut Vec<Act>,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(6.0);
        let track = pad_source[i]
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("clip");
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("EDIT  {track}"))
                    .color(AMBER)
                    .strong(),
            );
            let playing = mixer.pad_is_sounding(i);
            if ui
                .button(if playing {
                    "⏸ stop"
                } else {
                    "▶ play selection"
                })
                .clicked()
            {
                acts.push(Act::AuditionSel(i));
            }
            if ui.button("export").clicked() {
                acts.push(Act::ExportPad(i));
            }
            if ui.button("done").clicked() {
                acts.push(Act::CloseClip);
            }
        });

        let len = mixer.pad_clip_frames(i).max(1);
        let (inp, out) = mixer.pad_trim(i);
        let secs = |f: usize| f as f64 / mixer.sample_rate().max(1) as f64;
        ui.label(
            egui::RichText::new(format!(
                "in {:.3}s   out {:.3}s   selection {:.3}s   ·  drag the ◀ ▶ handles",
                secs(inp),
                secs(out),
                secs(out.saturating_sub(inp))
            ))
            .weak(),
        );
        ui.add_space(8.0);

        // The waveform canvas.
        let width = ui.available_width();
        let height = (ui.available_height() - 16.0).clamp(80.0, 360.0);
        let (rect, _resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, GROUND);
        let mid = rect.center().y;
        let amp = rect.height() * 0.45;
        let x_of = |frame: usize| rect.left() + (frame as f32 / len as f32) * rect.width();

        let cols = rect.width() as usize;
        let peaks = mixer.pad_peaks(i, cols);
        for (c, (lo, hi)) in peaks.iter().enumerate() {
            let x = rect.left() + c as f32;
            let frame = (c as f32 / cols.max(1) as f32 * len as f32) as usize;
            let inside = frame >= inp && frame < out;
            let color = if inside {
                AMBER
            } else {
                GREEN.gamma_multiply(0.35)
            };
            painter.line_segment(
                [egui::pos2(x, mid - hi * amp), egui::pos2(x, mid - lo * amp)],
                egui::Stroke::new(1.0, color),
            );
        }

        // Draggable handles. A thin grab rect around each in/out line.
        for (is_out, frame) in [(false, inp), (true, out)] {
            let hx = x_of(frame);
            let handle = egui::Rect::from_min_max(
                egui::pos2(hx - 5.0, rect.top()),
                egui::pos2(hx + 5.0, rect.bottom()),
            );
            let id = ui.id().with(("ce_handle", is_out));
            let resp = ui.interact(handle, id, egui::Sense::drag());
            let painter = ui.painter_at(rect);
            painter.line_segment(
                [egui::pos2(hx, rect.top()), egui::pos2(hx, rect.bottom())],
                egui::Stroke::new(2.0, AMBER),
            );
            let glyph = if is_out { "▶" } else { "◀" };
            painter.text(
                egui::pos2(hx, rect.top() + 8.0),
                egui::Align2::CENTER_TOP,
                glyph,
                egui::FontId::monospace(12.0),
                AMBER,
            );
            if resp.dragged() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let f = (((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * len as f32)
                        as usize;
                    if is_out {
                        acts.push(Act::SetTrimOut(i, f));
                    } else {
                        acts.push(Act::SetTrimIn(i, f));
                    }
                }
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_pad_cell(
    ui: &mut egui::Ui,
    mixer: &Mixer,
    pad_source: &[Option<PathBuf>; PADS],
    loading: &[bool; PADS],
    i: usize,
    w: f32,
    acts: &mut Vec<Act>,
) {
    let loaded = mixer.pad_loaded(i);
    let kind = match mixer.pad_kind(i) {
        PadKind::OneShot => "1shot",
        PadKind::Loop => "loop",
        PadKind::Scratch => "scratch",
    };
    let track = pad_source[i]
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let frame = egui::Frame::group(ui.style())
        .fill(PANEL)
        .stroke(egui::Stroke::new(
            1.0,
            if loaded {
                AMBER
            } else {
                GREEN.gamma_multiply(0.4)
            },
        ));
    let _ = kind;
    let (inner, payload) = ui.dnd_drop_zone::<DragTrack, _>(frame, |ui| {
        ui.set_width(w - 16.0);
        ui.set_min_height(104.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if loaded {
                    let btn = if mixer.pad_is_sounding(i) {
                        "⏸"
                    } else {
                        "▶"
                    };
                    if ui.button(btn).on_hover_text("play / pause").clicked() {
                        acts.push(Act::PlayPad(i));
                    }
                }
                let head = if track.is_empty() {
                    format!("{}", i + 1)
                } else {
                    format!("{}  {track}", i + 1)
                };
                ui.label(egui::RichText::new(head).color(AMBER).strong());
            });

            if loading[i] {
                ui.label(egui::RichText::new("⏳ loading…").color(GREEN));
                return;
            }
            if !loaded {
                ui.label(egui::RichText::new("drag a track here").weak());
                return;
            }

            // Kind selector (click to set — clearer than a drag for a 3-way toggle).
            ui.horizontal(|ui| {
                let k = mixer.pad_kind(i);
                for (label, want) in [
                    ("1shot", PadKind::OneShot),
                    ("loop", PadKind::Loop),
                    ("scratch", PadKind::Scratch),
                ] {
                    if ui.selectable_label(k == want, label).clicked() {
                        acts.push(Act::SetKind(i, want));
                    }
                }
            });

            // Volume.
            let mut g = mixer.pad_gain(i);
            if ui
                .add(
                    egui::Slider::new(&mut g, 0.0..=1.5)
                        .text("vol")
                        .show_value(false),
                )
                .changed()
            {
                acts.push(Act::SetGain(i, g));
            }

            ui.horizontal(|ui| {
                let mut on = mixer.pad_active(i);
                if ui.checkbox(&mut on, "on").changed() {
                    acts.push(Act::SetActive(i, on));
                }
                if ui.button("edit").on_hover_text("trim the clip").clicked() {
                    acts.push(Act::EditClip(i));
                }
                if ui.button("clear").clicked() {
                    acts.push(Act::ClearPad(i));
                }
                if ui
                    .button("export")
                    .on_hover_text("save trimmed clip to library (WAV)")
                    .clicked()
                {
                    acts.push(Act::ExportPad(i));
                }
            });
        });
    });
    let _ = inner;
    if let Some(p) = payload {
        acts.push(Act::LoadToPad {
            pad: i,
            path: p.0.clone(),
        });
    }
}
