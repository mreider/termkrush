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

// The landing-page palette (index.html CSS vars), so the app matches the site.
// Body text is the cream `--ink`; amber/green are accents, not the text color.
const INK: egui::Color32 = egui::Color32::from_rgb(0xe9, 0xe7, 0xd6); // --ink (primary text)
const GREEN: egui::Color32 = egui::Color32::from_rgb(0x45, 0xf0, 0x7d); // --green (accent)
const AMBER: egui::Color32 = egui::Color32::from_rgb(0xff, 0xb0, 0x00); // --amber (accent)
const DIM: egui::Color32 = egui::Color32::from_rgb(0x7e, 0x8c, 0x7f); // --dim (muted)
const RED: egui::Color32 = egui::Color32::from_rgb(0xff, 0x52, 0x52);
const GROUND: egui::Color32 = egui::Color32::from_rgb(0x06, 0x09, 0x07); // --bg
const PANEL: egui::Color32 = egui::Color32::from_rgb(0x16, 0x1b, 0x16); // panel fill
const LINE: egui::Color32 = egui::Color32::from_rgb(0x1d, 0x27, 0x1f); // --line (borders)

/// Waveform cache resolution (peak pairs). Computed once per clip, mapped to the
/// display width each frame — cheap, so audio is never starved by redrawing.
const WAVE_COLS: usize = 1600;

/// Launch the desktop app. Blocks until the window closes.
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // Blank titlebar text — the in-app wordmark is the brand.
            .with_title("")
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

/// A finished background decode on its way to the audio engine. `audio` is
/// `None` if the decode failed (so the in-flight count still settles).
struct LoadDone {
    target: Target,
    audio: Option<DecodedAudio>,
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
    /// The selected library track (delete / preview act on it).
    lib_sel: Option<PathBuf>,
    /// Inline rename in progress: `(target, buffer)`.
    renaming: Option<(PathBuf, String)>,
    /// Inline new-folder name being typed.
    new_folder: Option<String>,
    /// Source loaded on the scratch platter (for its label).
    jog_source: Option<PathBuf>,
    /// The library track currently previewing (so its row shows a stop button).
    previewing: Option<PathBuf>,
    /// Whether the preview has actually started sounding (to clear `previewing`
    /// only after it finishes, not during the decode gap).
    preview_was_on: bool,
    /// The pad whose clip is open in the editor (central-panel mode).
    clip_edit: Option<usize>,
    /// Cached clip-editor waveform `(pad, peaks)` — computed once on open so the
    /// whole-clip downsample doesn't run every frame (which starved the audio).
    clip_wave: Option<(usize, Vec<(f32, f32)>)>,
    /// Cached scratch-platter waveform peaks (computed once when armed).
    jog_wave: Vec<(f32, f32)>,
    /// How many background decodes are in flight (drives the loading overlay).
    pending_decodes: usize,

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
        install_fonts(&cc.egui_ctx);
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
            lib_sel: None,
            renaming: None,
            new_folder: None,
            jog_source: None,
            previewing: None,
            preview_was_on: false,
            clip_edit: None,
            clip_wave: None,
            jog_wave: Vec::new(),
            pending_decodes: 0,
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
    fn spawn_load(&mut self, target: Target, path: PathBuf) {
        self.pending_decodes += 1;
        let tx = self.load_tx.clone();
        let rate = self.target_rate;
        std::thread::spawn(move || {
            let done = match decode_file(&path, rate) {
                Ok(audio) => {
                    let bpm = matches!(target, Target::Pad(_))
                        .then(|| detect_bpm(&audio.samples, audio.channels, audio.sample_rate))
                        .flatten();
                    LoadDone {
                        target,
                        audio: Some(audio),
                        bpm,
                        path,
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, path = %path.display(), "decode failed");
                    LoadDone {
                        target,
                        audio: None,
                        bpm: None,
                        path,
                    }
                }
            };
            let _ = tx.send(done);
        });
    }

    /// Drain finished decodes into the engine.
    fn drain_loads(&mut self) {
        while let Ok(done) = self.load_rx.try_recv() {
            self.pending_decodes = self.pending_decodes.saturating_sub(1);
            let Some(audio) = done.audio else {
                continue; // decode failed; the overlay clears via the counter
            };
            match done.target {
                Target::Pad(i) => {
                    self.mixer.assign_pad(i, audio.samples);
                    if let Some(b) = done.bpm {
                        self.mixer.set_pad_bpm(i, Some(b));
                        // Auto-BPM: the first track to carry a tempo sets the master.
                        if self.mixer.master_bpm().is_none() {
                            self.mixer.set_master_bpm(Some(b));
                        }
                    }
                    self.pad_source[i] = Some(done.path);
                }
                Target::Preview => self.mixer.preview(audio.samples),
                Target::Jog => {
                    self.mixer.set_jog_source(audio.samples);
                    self.jog_wave = self.mixer.jog_peaks(WAVE_COLS);
                }
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
                if self.previewing.as_deref() == Some(p.as_path()) {
                    // Clicking the playing track's button stops it.
                    self.mixer.stop_preview();
                    self.previewing = None;
                    self.preview_was_on = false;
                } else {
                    // Switch the preview to this track.
                    self.mixer.stop_preview();
                    self.spawn_load(Target::Preview, p.clone());
                    self.previewing = Some(p);
                    self.preview_was_on = false;
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
                // Playing a pad ends any library audition — they shouldn't overlap.
                self.mixer.stop_preview();
                self.previewing = None;
                if self.mixer.pad_is_sounding(i) {
                    self.mixer.stop_pad(i);
                } else {
                    self.mixer.trigger_pad(i);
                }
            }
            Act::SetKind(i, k) => self.mixer.set_pad_kind(i, k),
            Act::SetGain(i, v) => self.mixer.set_pad_gain(i, v),
            Act::ClearPad(i) => {
                self.mixer.unload_pad(i);
                self.pad_source[i] = None;
            }
            Act::ExportPad(i) => self.export_pad(i),
            Act::EditClip(i) => {
                self.mixer.stop_pad(i); // silence live playback before editing
                self.clip_edit = Some(i);
                // Downsample the whole clip ONCE; reused every frame while editing.
                self.clip_wave = Some((i, self.mixer.pad_peaks(i, WAVE_COLS)));
            }
            Act::CloseClip => {
                if let Some(i) = self.clip_edit.take() {
                    self.mixer.stop_pad(i);
                }
                self.clip_wave = None;
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
            .exact_height(180.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(bungee("scratch", 14.0, AMBER));
                    let name = self
                        .jog_source
                        .as_ref()
                        .and_then(|p| p.file_stem())
                        .and_then(|s| s.to_str())
                        .unwrap_or("drag a sound onto the platter");
                    ui.label(egui::RichText::new(name).color(GREEN));
                    if self.mixer.has_jog() && ui.small_button("clear").clicked() {
                        self.mixer.clear_jog();
                        self.jog_source = None;
                        self.jog_wave.clear();
                    }
                });
                ui.add_space(4.0);

                let len = self.mixer.jog_len();
                let pos = self.mixer.jog_position().unwrap_or(0.0);
                let frac = if len > 0 {
                    (pos / len as f64) as f32
                } else {
                    0.0
                };

                ui.horizontal(|ui| {
                    // --- the vinyl platter: a drop target + the scratch surface ---
                    let size = egui::vec2(110.0, 110.0);
                    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                    let p = ui.painter_at(rect);
                    let c = rect.center();
                    let r = 52.0;
                    p.circle_filled(c, r, egui::Color32::from_rgb(0x12, 0x16, 0x12));
                    for k in 1..6 {
                        p.circle_stroke(c, r * k as f32 / 6.0, egui::Stroke::new(1.0, LINE));
                    }
                    // The spinning marker: angle advances with the playhead so
                    // you can see it move as you scratch.
                    let turns = 8.0;
                    let ang = frac * std::f32::consts::TAU * turns - std::f32::consts::FRAC_PI_2;
                    p.line_segment(
                        [c, c + r * egui::vec2(ang.cos(), ang.sin())],
                        egui::Stroke::new(2.0, AMBER),
                    );
                    p.circle_filled(c, 7.0, AMBER); // label
                    let glowing = self.mixer.has_jog();
                    p.circle_stroke(
                        c,
                        r,
                        egui::Stroke::new(1.5, if glowing { AMBER } else { DIM }),
                    );

                    // Drag the platter to scratch; fixed reference so the feel
                    // doesn't change with the disc size.
                    const SCRUB_REF: f64 = 520.0;
                    let vel = if resp.dragged() && len > 0 {
                        let dx = resp.drag_delta().x as f64;
                        (dx * len as f64 / (SCRUB_REF * self.target_rate as f64 * dt)) as f32
                    } else {
                        key_vel
                    };
                    self.mixer.set_jog_velocity(vel);
                    if let Some(d) = resp.dnd_release_payload::<DragTrack>() {
                        self.jog_source = Some(d.0.clone());
                        self.spawn_load(Target::Jog, d.0.clone());
                    }

                    // --- the waveform, with a playhead ---
                    let wave = egui::vec2(ui.available_width(), 110.0);
                    let (wr, _) = ui.allocate_exact_size(wave, egui::Sense::hover());
                    let wp = ui.painter_at(wr);
                    wp.rect_filled(wr, 4.0, GROUND);
                    wp.rect_stroke(wr, 4.0, egui::Stroke::new(1.0, LINE));
                    if len > 0 && !self.jog_wave.is_empty() {
                        let cols = wr.width() as usize;
                        let mid = wr.center().y;
                        let amp = wr.height() * 0.42;
                        for x in 0..cols {
                            let (lo, hi) = self.jog_wave[x * self.jog_wave.len() / cols.max(1)];
                            let xf = wr.left() + x as f32;
                            wp.line_segment(
                                [
                                    egui::pos2(xf, mid - hi * amp),
                                    egui::pos2(xf, mid - lo * amp),
                                ],
                                egui::Stroke::new(1.0, DIM),
                            );
                        }
                        let px = wr.left() + frac * wr.width();
                        wp.line_segment(
                            [egui::pos2(px, wr.top()), egui::pos2(px, wr.bottom())],
                            egui::Stroke::new(2.0, AMBER),
                        );
                    } else {
                        wp.text(
                            wr.center(),
                            egui::Align2::CENTER_CENTER,
                            "drag a sound here · hold ← whip / → wiki",
                            egui::FontId::monospace(13.0),
                            DIM,
                        );
                    }
                });
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
    // Never persist egui state to disk — a stale cache must not survive a
    // rebuild and shadow the current theme/layout.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_crt_theme(ctx); // re-assert the palette every frame
        self.pump_audio();
        self.drain_loads();
        ctx.request_repaint(); // keep the audio ring fed in real time

        // Clear the preview indicator once it has played and finished (but not
        // during the decode gap before it starts).
        if self.mixer.is_previewing() {
            self.preview_was_on = true;
        } else if self.preview_was_on {
            self.previewing = None;
            self.preview_was_on = false;
        }

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
                lib_sel,
                renaming,
                new_folder,
                playable,
                clip_edit,
                clip_wave,
                previewing,
                ..
            } = self;
            draw_timeline_strip(ctx, mixer);
            draw_library(
                ctx,
                crate_lib,
                lib_sel,
                renaming,
                new_folder,
                playable,
                previewing.as_deref(),
                &mut acts,
            );
            // Central panel: the clip editor when one is open, else the pads.
            if let Some(i) = *clip_edit {
                let wave = clip_wave
                    .as_ref()
                    .filter(|(p, _)| *p == i)
                    .map(|(_, w)| w.as_slice())
                    .unwrap_or(&[]);
                draw_clip_editor(ctx, mixer, pad_source, i, wave, &mut acts);
            } else {
                draw_pad_grid(ctx, mixer, pad_source, &mut acts);
            }
        }
        for a in acts {
            self.apply(a);
        }

        if self.pending_decodes > 0 {
            draw_loading_overlay(ctx, self.pending_decodes);
        }
        crt_overlay(ctx); // faint scanlines + vignette, on top of everything
    }
}

/// A dimmed full-window overlay with a spinner while tracks are decoding, so a
/// slow load doesn't feel frozen. Non-blocking — it's just paint.
fn draw_loading_overlay(ctx: &egui::Context, n: usize) {
    let rect = ctx.screen_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("dim"),
    ))
    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(150));
    egui::Area::new("loading".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(44.0).color(AMBER));
                ui.add_space(8.0);
                let label = if n > 1 {
                    format!("loading {n} tracks…")
                } else {
                    "loading…".to_string()
                };
                ui.label(bungee(label, 14.0, INK));
            });
        });
}

/// Build the landing-page Visuals. Pure + testable so the palette can't silently
/// regress and so we can confirm the binary carries the new theme. Body text is
/// cream `INK` via the widget strokes (NOT `override_text_color` — overriding
/// flattened every label to one low-contrast colour, the "barely visible" bug).
fn crt_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.override_text_color = None;

    // Surfaces.
    v.panel_fill = GROUND;
    v.window_fill = PANEL;
    v.extreme_bg_color = GROUND;
    v.window_stroke = egui::Stroke::new(1.0, LINE);

    // Text: cream ink at rest, amber on hover/active. Widget fills are panels;
    // borders are the dim `--line`.
    let line = egui::Stroke::new(1.0, LINE);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.bg_fill = PANEL;
        w.weak_bg_fill = PANEL;
        w.bg_stroke = line;
        w.fg_stroke = egui::Stroke::new(1.0, INK);
    }
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, AMBER);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, AMBER);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, AMBER);

    v.selection.bg_fill = AMBER.gamma_multiply(0.30);
    v.selection.stroke = egui::Stroke::new(1.0, AMBER);
    v
}

/// Apply the palette + Space Mono body face. Cheap, called every frame so no
/// restored/default state can ever shadow the theme.
fn apply_crt_theme(ctx: &egui::Context) {
    ctx.set_visuals(crt_visuals());
    let mut style = (*ctx.style()).clone();
    for font in style.text_styles.values_mut() {
        font.family = egui::FontFamily::Monospace;
    }
    // Labels aren't text fields — no I-beam cursor / selection on them.
    style.interaction.selectable_labels = false;
    ctx.set_style(style);
}

/// A prominent painted close button (an amber X). Returns true on click. Used
/// everywhere we close a view, instead of a "done" word.
fn close_x(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    let col = if resp.hovered() { AMBER } else { DIM };
    let p = ui.painter_at(rect);
    p.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, col));
    let m = 8.0;
    let (tl, br) = (rect.left_top(), rect.right_bottom());
    p.line_segment(
        [tl + egui::vec2(m, m), br - egui::vec2(m, m)],
        egui::Stroke::new(2.0, col),
    );
    p.line_segment(
        [
            egui::pos2(br.x - m, tl.y + m),
            egui::pos2(tl.x + m, br.y - m),
        ],
        egui::Stroke::new(2.0, col),
    );
    resp.on_hover_cursor(egui::CursorIcon::Default).clicked()
}

/// Small painted "new folder" button (a folder tab + a +). Returns true on click.
fn folder_plus_button(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 22.0), egui::Sense::click());
    let col = if resp.hovered() { AMBER } else { DIM };
    let p = ui.painter_at(rect);
    let r = rect.shrink(4.0);
    // folder body + tab
    p.rect_stroke(
        egui::Rect::from_min_max(egui::pos2(r.left(), r.top() + 3.0), r.right_bottom()),
        1.0,
        egui::Stroke::new(1.0, col),
    );
    p.line_segment(
        [
            egui::pos2(r.left(), r.top() + 3.0),
            egui::pos2(r.left() + 6.0, r.top()),
        ],
        egui::Stroke::new(1.0, col),
    );
    // plus
    let c = r.center() + egui::vec2(0.0, 2.0);
    p.line_segment(
        [c - egui::vec2(3.0, 0.0), c + egui::vec2(3.0, 0.0)],
        egui::Stroke::new(1.5, col),
    );
    p.line_segment(
        [c - egui::vec2(0.0, 3.0), c + egui::vec2(0.0, 3.0)],
        egui::Stroke::new(1.5, col),
    );
    resp.on_hover_cursor(egui::CursorIcon::Default)
        .on_hover_text("new folder")
        .clicked()
}

/// Small painted trash can that is a drop target (drag a track here to delete)
/// and clickable (delete the selection). Returns `(clicked, dropped_payload)`.
fn trash_zone(ui: &mut egui::Ui) -> (bool, Option<std::sync::Arc<DragTrack>>) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 22.0), egui::Sense::click());
    let dropped = resp.dnd_release_payload::<DragTrack>();
    let hovering = resp.dnd_hover_payload::<DragTrack>().is_some();
    let col = if hovering || resp.hovered() { RED } else { DIM };
    let p = ui.painter_at(rect);
    let r = rect.shrink(5.0);
    // lid + handle
    p.line_segment(
        [
            egui::pos2(r.left() - 1.0, r.top()),
            egui::pos2(r.right() + 1.0, r.top()),
        ],
        egui::Stroke::new(1.5, col),
    );
    p.line_segment(
        [
            egui::pos2(r.center().x - 3.0, r.top() - 2.0),
            egui::pos2(r.center().x + 3.0, r.top() - 2.0),
        ],
        egui::Stroke::new(1.5, col),
    );
    // can body (slightly tapered) + ribs
    p.rect_stroke(
        egui::Rect::from_min_max(egui::pos2(r.left() + 1.0, r.top() + 3.0), r.right_bottom()),
        1.0,
        egui::Stroke::new(1.0, col),
    );
    for dx in [-2.5, 0.0, 2.5] {
        let x = r.center().x + dx;
        p.line_segment(
            [
                egui::pos2(x, r.top() + 6.0),
                egui::pos2(x, r.bottom() - 2.0),
            ],
            egui::Stroke::new(1.0, col),
        );
    }
    if hovering {
        ui.painter()
            .rect_stroke(rect, 3.0, egui::Stroke::new(1.5, RED));
    }
    (
        resp.on_hover_cursor(egui::CursorIcon::Default)
            .on_hover_text("drag here / click to delete")
            .clicked(),
        dropped,
    )
}

/// Bundle the brand fonts: Space Mono for body/UI, Bungee for the wordmark.
/// Called once at startup.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "spacemono".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/SpaceMono-Regular.ttf")),
    );
    fonts.font_data.insert(
        "bungee".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Bungee-Regular.ttf")),
    );
    // Space Mono is the default for both families (it reads as the site's body).
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(fam)
            .or_default()
            .insert(0, "spacemono".to_owned());
    }
    // Bungee is a named family for the wordmark.
    fonts.families.insert(
        egui::FontFamily::Name("bungee".into()),
        vec!["bungee".to_owned()],
    );
    ctx.set_fonts(fonts);
}

/// A volume control with a *visible* track: dim rail, amber fill, a knob.
/// Returns the new value while dragging. `0.0..=1.5` (1.0 = unity).
fn vol_slider(ui: &mut egui::Ui, value: f32) -> Option<f32> {
    let w = ui.available_width().clamp(60.0, 150.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 16.0), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect);
    let y = rect.center().y;
    p.line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(2.0, LINE),
    );
    let t = (value / 1.5).clamp(0.0, 1.0);
    let knob_x = rect.left() + t * rect.width();
    p.line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(knob_x, y)],
        egui::Stroke::new(2.0, AMBER),
    );
    p.circle_filled(egui::pos2(knob_x, y), 5.0, AMBER);
    p.circle_stroke(egui::pos2(knob_x, y), 5.0, egui::Stroke::new(1.0, GROUND));
    if (resp.dragged() || resp.clicked()) && resp.interact_pointer_pos().is_some() {
        let px = resp.interact_pointer_pos().unwrap().x;
        let nt = ((px - rect.left()) / rect.width()).clamp(0.0, 1.0);
        return Some(nt * 1.5);
    }
    None
}

/// Wordmark text in the Bungee display face.
fn bungee(text: impl Into<String>, size: f32, color: egui::Color32) -> egui::RichText {
    egui::RichText::new(text)
        .font(egui::FontId::new(
            size,
            egui::FontFamily::Name("bungee".into()),
        ))
        .color(color)
}

/// Subtle CRT atmosphere over everything: faint scanlines + a corner vignette.
/// Kept low-alpha so it's mood, not noise (readability stays first).
fn crt_overlay(ctx: &egui::Context) {
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crt"),
    ));
    let scan = egui::Color32::from_black_alpha(16);
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, scan),
        );
        y += 3.0;
    }
    // Light vignette: a darker stroke hugging the window edge.
    painter.rect_stroke(
        rect.shrink(1.0),
        0.0,
        egui::Stroke::new(2.0, egui::Color32::from_black_alpha(60)),
    );
}

fn draw_timeline_strip(ctx: &egui::Context, mixer: &Mixer) {
    egui::TopBottomPanel::top("timeline")
        .exact_height(72.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                // Brand: a little amber vinyl + the Bungee wordmark.
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                let p = ui.painter_at(rect);
                p.circle_filled(rect.center(), 10.0, PANEL);
                p.circle_stroke(rect.center(), 10.0, egui::Stroke::new(1.0, LINE));
                p.circle_filled(rect.center(), 3.0, AMBER);
                ui.add_space(2.0);
                ui.label(bungee("termkrush", 18.0, AMBER));
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
    previewing: Option<&Path>,
    acts: &mut Vec<Act>,
) {
    egui::SidePanel::left("library")
        .resizable(true)
        .default_width(260.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(bungee("library", 14.0, AMBER));
                // Right-aligned icon controls: a trash drop target + new-folder.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (clicked, dropped) = trash_zone(ui);
                    if let Some(d) = dropped {
                        acts.push(Act::Delete(d.0.clone()));
                    } else if clicked {
                        if let Some(p) = sel {
                            acts.push(Act::Delete(p.clone()));
                        }
                    }
                    if folder_plus_button(ui) {
                        acts.push(Act::StartNewFolder);
                    }
                });
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
                // Up a level — click to go up, or drop a track here to move it
                // out to the parent folder.
                if lib.cwd() != lib.root() {
                    if let Some(parent) = lib.cwd().parent() {
                        draw_folder_row(ui, ".. (up)", parent, acts);
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
                        let playing = previewing == Some(e.path.as_path());
                        draw_track_row(ui, e, sel, renaming, bad, playing, acts);
                    }
                }
                if lib.is_empty() {
                    ui.label(egui::RichText::new("(empty — set crate_root)").weak());
                }
            });
        });
}

/// A folder row: click to open, a drop target to move a track in. Highlights
/// amber while a track is dragged over it, so the drop target is obvious.
fn draw_folder_row(ui: &mut egui::Ui, name: &str, path: &Path, acts: &mut Vec<Act>) {
    let label = if name.ends_with(')') {
        name.to_string() // ".. (up)" — leave as-is
    } else {
        format!("{name}/")
    };
    // A clickable label (a dnd drop zone alone doesn't sense clicks, which is
    // why folders wouldn't open) that is also a drop target for moving in.
    let resp = ui
        .add(
            egui::Label::new(egui::RichText::new(label).color(AMBER).strong())
                .sense(egui::Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::Default);
    if resp.dnd_hover_payload::<DragTrack>().is_some() {
        let r = resp.rect.expand2(egui::vec2(4.0, 2.0));
        ui.painter().rect_filled(r, 3.0, AMBER.gamma_multiply(0.18));
        ui.painter()
            .rect_stroke(r, 3.0, egui::Stroke::new(1.5, AMBER));
    }
    if resp.clicked() {
        acts.push(Act::EnterFolder(path.to_path_buf()));
    }
    if let Some(p) = resp.dnd_release_payload::<DragTrack>() {
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
    playing: bool,
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
    ui.horizontal(|ui| {
        // Per-row play/stop button (preview). ■ while this track is playing.
        let glyph = if playing { "■" } else { "▶" };
        let btn =
            egui::Button::new(egui::RichText::new(glyph).color(if playing { AMBER } else { INK }))
                .small()
                .frame(false);
        if ui
            .add_enabled(!bad, btn)
            .on_hover_text("play / stop")
            .clicked()
        {
            acts.push(Act::Preview(e.path.clone()));
        }
        // The name is the drag source (load/move); click selects, dbl-click renames.
        let id = egui::Id::new(("track", &e.path));
        let resp = ui
            .dnd_drag_source(id, DragTrack(e.path.clone()), |ui| {
                let mut text = egui::RichText::new(&e.name);
                if bad {
                    text = text.color(RED);
                } else if selected {
                    text = text.color(AMBER).strong();
                } else {
                    text = text.color(INK);
                }
                let label = ui.label(text);
                if bad {
                    label.on_hover_text("unplayable — won't decode");
                }
            })
            .response
            .on_hover_cursor(egui::CursorIcon::Default);
        if selected {
            ui.painter().rect_stroke(
                resp.rect.expand(1.0),
                2.0,
                egui::Stroke::new(1.0, AMBER.gamma_multiply(0.7)),
            );
        }
        if resp.clicked() {
            acts.push(Act::Select(e.path.clone()));
        }
        if resp.double_clicked() {
            acts.push(Act::StartRename(e.path.clone()));
        }
    });
}

fn draw_pad_grid(
    ctx: &egui::Context,
    mixer: &Mixer,
    pad_source: &[Option<PathBuf>; PADS],
    acts: &mut Vec<Act>,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(bungee("pads", 14.0, AMBER));
            ui.label(egui::RichText::new("drag a track onto a pad to load").color(DIM));
        });
        ui.add_space(6.0);
        // `ui.columns` gives equal-width columns regardless of content, so the
        // cells are uniform; names truncate to fit.
        const COLS: usize = 4;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for row in 0..PADS.div_ceil(COLS) {
                ui.columns(COLS, |cols| {
                    for (c, col) in cols.iter_mut().enumerate() {
                        let i = row * COLS + c;
                        if i < PADS {
                            draw_pad_cell(col, mixer, pad_source, i, acts);
                        }
                    }
                });
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
    wave: &[(f32, f32)],
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
            // Just the track name — you can see you're editing; no "EDIT" label.
            ui.label(bungee(track, 16.0, AMBER));
            let playing = mixer.pad_is_sounding(i);
            if ui
                .button(if playing { "■ stop" } else { "▶ play" })
                .clicked()
            {
                acts.push(Act::AuditionSel(i));
            }
            if ui.button("export").clicked() {
                acts.push(Act::ExportPad(i));
            }
            // Prominent X to close, top-right.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if close_x(ui) {
                    acts.push(Act::CloseClip);
                }
            });
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
        for c in 0..cols.min(if wave.is_empty() { 0 } else { cols }) {
            let (lo, hi) = wave[c * wave.len() / cols.max(1)];
            let x = rect.left() + c as f32;
            let frame = (c as f32 / cols.max(1) as f32 * len as f32) as usize;
            let inside = frame >= inp && frame < out;
            let color = if inside { AMBER } else { DIM };
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
    i: usize,
    acts: &mut Vec<Act>,
) {
    let loaded = mixer.pad_loaded(i);
    let track = pad_source[i]
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let frame = egui::Frame::group(ui.style())
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, if loaded { AMBER } else { LINE }));
    let (inner, payload) = ui.dnd_drop_zone::<DragTrack, _>(frame, |ui| {
        ui.set_width(ui.available_width()); // fill the equal column → uniform cells
        ui.set_min_height(96.0);
        ui.vertical(|ui| {
            // Header: play/pause + the (truncated) track name. No pad number.
            ui.horizontal(|ui| {
                if loaded {
                    let btn = if mixer.pad_is_sounding(i) {
                        "■"
                    } else {
                        "▶"
                    };
                    if ui.button(btn).on_hover_text("play / pause").clicked() {
                        acts.push(Act::PlayPad(i));
                    }
                }
                let name = if track.is_empty() { "—" } else { &track };
                ui.add(
                    egui::Label::new(egui::RichText::new(name).color(AMBER).strong()).truncate(),
                );
            });

            if !loaded {
                ui.label(egui::RichText::new("drag a track here").weak());
                return;
            }

            // Kind selector (click to set).
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

            // Volume — visible amber track.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("vol").color(DIM).small());
                if let Some(v) = vol_slider(ui, mixer.pad_gain(i)) {
                    acts.push(Act::SetGain(i, v));
                }
            });

            // Actions — no "on" toggle (a loaded pad is live; volume mutes).
            ui.horizontal(|ui| {
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
    // Highlight the pad while a track is dragged over it, so it's clear it'll
    // load here on drop.
    if inner.response.dnd_hover_payload::<DragTrack>().is_some() {
        let r = inner.response.rect;
        ui.painter().rect_filled(r, 4.0, AMBER.gamma_multiply(0.15));
        ui.painter()
            .rect_stroke(r, 4.0, egui::Stroke::new(2.0, AMBER));
    }
    if let Some(p) = payload {
        acts.push(Act::LoadToPad {
            pad: i,
            path: p.0.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crt_palette_is_readable_not_overridden() {
        let v = crt_visuals();
        assert!(v.override_text_color.is_none(), "no global green override");
        assert_eq!(
            v.widgets.noninteractive.fg_stroke.color, INK,
            "body text is ink"
        );
        assert_eq!(v.widgets.inactive.fg_stroke.color, INK);
        assert_eq!(v.panel_fill, GROUND);
    }
}
