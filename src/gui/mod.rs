//! The egui/eframe desktop front-end — the sole UI (see the 2026-06-08 GUI
//! pivot in `.am/inception.md`). The engine (`termkrush-core`) is unchanged;
//! this is all view + input.
//!
//! Mouse model: drag a library track onto a clip to load it, into a folder to
//! move it, or onto a timeline lane to place it; drag a clip by its name to the
//! timeline; double-click to rename; click ▶ to preview. No modal dialogs —
//! inline fields and buttons.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

use eframe::egui;
use egui_phosphor::regular as ph;
use termkrush_core::arrangement::{Arrangement, Block, Phase};
use termkrush_core::audio::{
    decode_file, detect_bpm, probe_playable, write_wav, AudioOutput, DecodedAudio,
};
use termkrush_core::config::Config;
use termkrush_core::library::Crate;
use termkrush_core::mix::{Mixer, PadKind, PADS};
use termkrush_core::scratch::detect_pivot;

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
    let mut viewport = egui::ViewportBuilder::default()
        // Blank titlebar text — the in-app wordmark is the brand.
        .with_title("")
        .with_inner_size([1100.0, 720.0])
        .with_min_inner_size([720.0, 480.0]);
    // Window / dock / taskbar icon (the turntable mark).
    if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icons/termkrush-256.png"))
    {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
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

/// A pad being dragged onto the timeline (its trimmed clip becomes a block).
#[derive(Clone)]
struct DragPad(usize);

/// Where a background decode is headed when it lands.
#[derive(Clone, Copy)]
enum Target {
    Pad(usize),
    Preview,
    /// Arm the scratch platter.
    Jog,
    /// Place a block on timeline `track` starting at `start` frames.
    Timeline {
        track: usize,
        start: u64,
    },
}

/// A finished background decode on its way to the audio engine. `audio` is
/// `None` if the decode failed (so the in-flight count still settles).
struct LoadDone {
    target: Target,
    audio: Option<DecodedAudio>,
    bpm: Option<f32>,
    /// First-onset frame (timeline drops) so the block can phase-snap the hit.
    onset: u64,
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
    /// Spring-loaded folders: `(folder, hover-start time)` while a track is
    /// dragged over it — after a hold we navigate in so you can drop elsewhere.
    spring: Option<(PathBuf, f64)>,

    /// The free-track timeline arrangement.
    arrangement: Arrangement,
    /// Timeline transport: rolling + playhead position (frames).
    tl_playing: bool,
    tl_playhead: u64,
    /// Selected block on the timeline: `(track, index)` — for move/copy/delete.
    tl_sel: Option<(usize, usize)>,
    /// Copied block, for Cmd-V paste.
    tl_clip: Option<Block>,
    /// Timeline zoom (pixels per second) and horizontal scroll (left-edge frame).
    tl_pxps: f32,
    tl_scroll: u64,
    /// While moving a block: px offset from its left edge to the grab point, and
    /// which block is being dragged (so its original is hidden, ghost shown).
    tl_grab_dx: f32,
    tl_moving: Option<(usize, usize)>,

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
        let arrangement = Arrangement::new(rate, 4); // start with 4 tracks

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
            spring: None,
            arrangement,
            tl_playing: false,
            tl_playhead: 0,
            tl_sel: None,
            tl_clip: None,
            tl_pxps: 60.0,
            tl_scroll: 0,
            tl_grab_dx: 0.0,
            tl_moving: None,
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
        // The timeline arrangement plays on top of the live pads when rolling.
        if self.tl_playing {
            self.arrangement
                .mix_into(self.tl_playhead, &mut self.scratch);
            let total = self.arrangement.total_frames();
            self.tl_playhead += frames as u64;
            if total == 0 || self.tl_playhead >= total {
                self.tl_playhead = 0; // loop back to the top
            }
        }
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
                    // Detect tempo for pads AND timeline drops (so a library
                    // track dropped on the MASTER track can set the BPM).
                    let bpm = matches!(target, Target::Pad(_) | Target::Timeline { .. })
                        .then(|| detect_bpm(&audio.samples, audio.channels, audio.sample_rate))
                        .flatten();
                    // Onset (the musical hit) for timeline drops — the calc behind
                    // the loading spinner; the phase snap aligns this to the grid.
                    let onset = matches!(target, Target::Timeline { .. })
                        .then(|| detect_pivot(&audio.samples, audio.channels) as u64)
                        .unwrap_or(0);
                    LoadDone {
                        target,
                        audio: Some(audio),
                        bpm,
                        onset,
                        path,
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, path = %path.display(), "decode failed");
                    LoadDone {
                        target,
                        audio: None,
                        bpm: None,
                        onset: 0,
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
                        // Remember the clip's tempo, but the MASTER timeline track
                        // (not the first clip loaded) sets the master BPM.
                        self.mixer.set_pad_bpm(i, Some(b));
                    }
                    self.pad_source[i] = Some(done.path);
                }
                Target::Preview => self.mixer.preview(audio.samples),
                Target::Jog => {
                    self.mixer.set_jog_source(audio.samples);
                    self.jog_wave = self.mixer.jog_peaks(WAVE_COLS);
                }
                Target::Timeline { track, start } => {
                    let raw = start as f64; // un-snapped drop frame
                    let label = done
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("clip")
                        .to_string();
                    // MASTER track (track 0) sets the tempo FIRST, so the grid
                    // exists before we phase-snap this block onto it.
                    if track == 0 {
                        if let Some(b) = done.bpm {
                            self.mixer.set_master_bpm(Some(b));
                            self.arrangement.set_target_bpm(Some(b));
                        }
                    }
                    let mut block = Block {
                        samples: std::sync::Arc::new(audio.samples),
                        start: 0,
                        label,
                        source_pad: None, // a library drop has no live clip
                        bpm: done.bpm,
                        onset: done.onset,
                        phase: Phase::OnBeat,
                    };
                    let oo = self.onset_out(&block);
                    block.start = self.snapped_start(raw, oo, block.phase);
                    self.arrangement.add_block(track, block);
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
                    // Switch the preview to this track (and stop the timeline —
                    // preview and timeline playback are mutually exclusive).
                    self.tl_playing = false;
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
            Act::SetGain(i, v) => {
                self.mixer.set_pad_gain(i, v);
                self.refresh_pad_blocks(i);
            }
            Act::ClearPad(i) => {
                self.mixer.unload_pad(i);
                self.pad_source[i] = None;
                // Keep any placed blocks but cut their link to this pad.
                self.arrangement.unlink_pad(i);
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
            Act::SetTrimIn(i, f) => {
                self.mixer.set_pad_trim_in(i, f);
                self.refresh_pad_blocks(i);
            }
            Act::SetTrimOut(i, f) => {
                self.mixer.set_pad_trim_out(i, f);
                self.refresh_pad_blocks(i);
            }
            Act::AuditionSel(i) => {
                if self.mixer.pad_is_sounding(i) {
                    self.mixer.stop_pad(i);
                } else {
                    self.tl_playing = false; // auditioning a clip stops the timeline
                    let (inp, out) = self.mixer.pad_trim(i);
                    self.mixer.audition_region(i, inp, out);
                }
            }
        }
    }

    /// The timeline (top panel): brand + transport, then free tracks of blocks.
    /// Drag a library track onto a lane to place a block (snapped to the beat);
    /// click a block to select, Delete to remove, Cmd-C/V to copy/paste.
    fn draw_timeline(&mut self, ctx: &egui::Context) {
        const LANE_H: f32 = 34.0;
        const GUTTER: f32 = 10.0; // left margin so frame 0 isn't flush to the edge
        let sr = self.target_rate.max(1) as f32;
        let master_bpm = self.mixer.master_bpm();
        // Placement snapping now goes through `snapped_start` (onset + phase).

        egui::TopBottomPanel::top("timeline")
            .exact_height(220.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                // --- brand + transport ---
                ui.horizontal(|ui| {
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                    let p = ui.painter_at(r);
                    p.circle_filled(r.center(), 10.0, PANEL);
                    p.circle_stroke(r.center(), 10.0, egui::Stroke::new(1.0, LINE));
                    p.circle_filled(r.center(), 3.0, AMBER);
                    ui.label(bungee("termkrush", 18.0, AMBER));
                    ui.add_space(12.0);
                    if icon_btn(
                        ui,
                        if self.tl_playing { ph::PAUSE } else { ph::PLAY },
                        "play / pause the timeline",
                    ) {
                        self.tl_playing = !self.tl_playing;
                        if self.tl_playing {
                            self.mixer.stop_preview();
                            self.previewing = None;
                            for pad in 0..PADS {
                                self.mixer.stop_pad(pad);
                            }
                        }
                    }
                    if icon_btn(ui, ph::STOP, "stop (rewind to start)") {
                        self.tl_playing = false;
                        self.tl_playhead = 0;
                        self.tl_scroll = 0;
                    }
                    if icon_btn(ui, ph::FLOPPY_DISK, "render the mix to the library (WAV)") {
                        self.render_arrangement();
                    }
                    if icon_btn(ui, ph::PLUS, "add a track") {
                        self.arrangement.add_track();
                    }
                    ui.add_space(8.0);
                    // Zoom + horizontal scroll.
                    if icon_btn(ui, ph::MAGNIFYING_GLASS_MINUS, "zoom out") {
                        self.tl_pxps = (self.tl_pxps / 1.4).max(8.0);
                    }
                    if icon_btn(ui, ph::MAGNIFYING_GLASS_PLUS, "zoom in") {
                        self.tl_pxps = (self.tl_pxps * 1.4).min(400.0);
                    }
                    let step = (sr * 2.0) as u64;
                    if icon_btn(ui, ph::CARET_LEFT, "scroll left") {
                        self.tl_scroll = self.tl_scroll.saturating_sub(step);
                    }
                    if icon_btn(ui, ph::CARET_RIGHT, "scroll right") {
                        self.tl_scroll += step;
                    }
                    ui.add_space(8.0);
                    let bpm = master_bpm
                        .map(|b| format!("{b:.0} BPM"))
                        .unwrap_or_else(|| "-- BPM".into());
                    ui.label(egui::RichText::new(bpm).color(GREEN));
                    // Where the playhead is: bar.beat (if tempo) or m:ss.
                    let ph = self.tl_playhead;
                    let pos = match master_bpm {
                        Some(b) if b > 0.0 => {
                            let fpb = (sr * 60.0 / b) as u64; // frames / beat
                            let beat = ph.checked_div(fpb).unwrap_or(0);
                            format!("{}.{}", beat / 4 + 1, beat % 4 + 1)
                        }
                        _ => {
                            let s = (ph as f32 / sr) as u64;
                            format!("{}:{:02}", s / 60, s % 60)
                        }
                    };
                    ui.label(egui::RichText::new(pos).color(DIM));
                });
                ui.add_space(4.0);

                let width = ui.available_width();
                let pxps = self.tl_pxps;
                // Mouse wheel scrolls the timeline horizontally when hovered.
                if ui.rect_contains_pointer(ui.max_rect()) {
                    let dx = ui.input(|i| i.smooth_scroll_delta.x + i.smooth_scroll_delta.y);
                    if dx.abs() > 0.0 {
                        let df = (dx / pxps * sr) as i64;
                        self.tl_scroll = (self.tl_scroll as i64 - df).max(0) as u64;
                    }
                }
                // Page the view when the playhead crosses the right edge, so it
                // restarts from the left rather than scrolling continuously. Only
                // while PLAYING — otherwise it yanks back any manual scroll.
                let view_frames = (((width - GUTTER).max(1.0)) / pxps * sr) as u64;
                if self.tl_playing
                    && (self.tl_playhead >= self.tl_scroll + view_frames
                        || self.tl_playhead < self.tl_scroll)
                {
                    self.tl_scroll = self.tl_playhead;
                }

                // Only scrollable when the content is longer than the view. When
                // it is, the extent adds half a screen of headroom (so you can
                // drop just past the end); otherwise there's nothing to scroll.
                let content = self.arrangement.total_frames();
                let scrollable = content > view_frames;
                let extent = if scrollable {
                    content + view_frames / 2
                } else {
                    view_frames.max(1)
                };
                let max_scroll = extent.saturating_sub(view_frames);
                self.tl_scroll = self.tl_scroll.min(max_scroll);

                let scroll = self.tl_scroll as f64;
                let x_of = |left: f32, frame: u64| {
                    left + GUTTER + ((frame as f64 - scroll) as f32 / sr * pxps)
                };
                let frame_at = |left: f32, x: f32| -> u64 {
                    (scroll + ((x - left - GUTTER) / pxps * sr) as f64).max(0.0) as u64
                };

                // --- ruler: scrub the playhead + read where you are ---
                let (ruler, rresp) =
                    ui.allocate_exact_size(egui::vec2(width, 22.0), egui::Sense::click_and_drag());
                let rp = ui.painter_at(ruler);
                rp.rect_filled(ruler, 0.0, GROUND);
                rp.line_segment(
                    [ruler.left_bottom(), ruler.right_bottom()],
                    egui::Stroke::new(1.0, LINE),
                );
                let tick = |x: f32, label: Option<String>| {
                    if x >= ruler.left() && x <= ruler.right() {
                        rp.line_segment(
                            [
                                egui::pos2(x, ruler.bottom() - 5.0),
                                egui::pos2(x, ruler.bottom()),
                            ],
                            egui::Stroke::new(1.0, DIM),
                        );
                        if let Some(s) = label {
                            // Centered over the tick so the number lines up with it.
                            rp.text(
                                egui::pos2(x, ruler.top() + 1.0),
                                egui::Align2::CENTER_TOP,
                                s,
                                egui::FontId::proportional(9.0),
                                DIM,
                            );
                        }
                    }
                };
                match master_bpm {
                    // Bars (4/4) when a tempo is known; label every Nth so they
                    // don't crowd at low zoom.
                    Some(b) if b > 0.0 => {
                        let fpbar = sr * 60.0 / b * 4.0;
                        let step = ((40.0 / (fpbar / sr * pxps)).ceil() as u32).max(1);
                        let mut bar = 0u32;
                        loop {
                            let x = x_of(ruler.left(), (bar as f64 * fpbar as f64) as u64);
                            if x > ruler.right() {
                                break;
                            }
                            tick(x, (bar % step == 0).then(|| format!("{}", bar + 1)));
                            bar += 1;
                            if bar > 8192 {
                                break;
                            }
                        }
                    }
                    // Otherwise seconds (m:ss).
                    _ => {
                        let step = ((48.0 / pxps).ceil() as u32).max(1);
                        let mut s = 0u32;
                        loop {
                            let x = x_of(ruler.left(), (s as f64 * sr as f64) as u64);
                            if x > ruler.right() {
                                break;
                            }
                            tick(
                                x,
                                (s % step == 0).then(|| format!("{}:{:02}", s / 60, s % 60)),
                            );
                            s += 1;
                            if s > 100_000 {
                                break;
                            }
                        }
                    }
                }
                if rresp.clicked() || rresp.dragged() {
                    if let Some(pp) = rresp.interact_pointer_pos() {
                        self.tl_playhead = frame_at(ruler.left(), pp.x);
                    }
                }
                let phx = x_of(ruler.left(), self.tl_playhead);
                if phx >= ruler.left() && phx <= ruler.right() {
                    // Painted downward triangle (a glyph would be tofu in our fonts).
                    rp.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(phx - 5.0, ruler.top()),
                            egui::pos2(phx + 5.0, ruler.top()),
                            egui::pos2(phx, ruler.top() + 9.0),
                        ],
                        GREEN,
                        egui::Stroke::NONE,
                    ));
                }

                // --- track lanes --- (collect into locals; apply after)
                let moving_prev = self.tl_moving;
                let target_bpm = self.arrangement.target_bpm(); // for synced block widths
                let mut grab_dx = self.tl_grab_dx;
                // Length (frames) of a clip being dragged from the grid, so we
                // can preview the real block shape on the lane it's over.
                let drag_preview_len: Option<u64> = egui::DragAndDrop::payload::<DragPad>(ui.ctx())
                    .map(|pd| {
                        let (a, b) = self.mixer.pad_trim(pd.0);
                        let native = b.saturating_sub(a) as f64;
                        // Show the varispeed (synced) length, matching where it lands.
                        let speed = match (self.mixer.pad_bpm(pd.0), target_bpm) {
                            (Some(bp), Some(t)) if bp > 0.0 && t > 0.0 => (t / bp) as f64,
                            _ => 1.0,
                        };
                        (native / speed).round() as u64
                    });
                let mut drop: Option<(usize, u64, PathBuf)> = None;
                let mut drop_pad: Option<(usize, u64, usize)> = None;
                let mut clicked: Option<(usize, usize)> = None;
                let mut cycle_phase: Option<(usize, usize)> = None;
                let mut moving: Option<(usize, usize, egui::Pos2)> = None;
                let mut move_commit: Option<(usize, usize, egui::Pos2)> = None;
                let mut lane_rects: Vec<egui::Rect> = Vec::new();
                egui::ScrollArea::vertical()
                    .max_height(138.0)
                    .show(ui, |ui| {
                        let tracks = self.arrangement.track_count();
                        for t in 0..tracks {
                            let (lane, _resp) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), LANE_H),
                                egui::Sense::hover(),
                            );
                            lane_rects.push(lane);
                            let p = ui.painter_at(lane); // clips drawing to this lane
                            let is_master = t == 0;
                            p.rect_filled(
                                lane,
                                3.0,
                                if is_master {
                                    AMBER.gamma_multiply(0.08)
                                } else {
                                    PANEL
                                },
                            );
                            p.rect_stroke(
                                lane,
                                3.0,
                                egui::Stroke::new(1.0, if is_master { AMBER } else { LINE }),
                            );
                            if is_master {
                                p.text(
                                    lane.right_top() + egui::vec2(-6.0, 2.0),
                                    egui::Align2::RIGHT_TOP,
                                    "MASTER",
                                    egui::FontId::proportional(9.0),
                                    AMBER.gamma_multiply(0.8),
                                );
                            }
                            for (bi, block) in
                                self.arrangement.tracks()[t].blocks.iter().enumerate()
                            {
                                let x0 = x_of(lane.left(), block.start);
                                let x1 = x_of(lane.left(), block.end_at(target_bpm));
                                let br = egui::Rect::from_min_max(
                                    egui::pos2(x0, lane.top() + 2.0),
                                    egui::pos2(x1.max(x0 + 3.0), lane.bottom() - 2.0),
                                );
                                // Phase badge sits at the top-right (when there's room).
                                let badge = (br.width() > 34.0).then(|| {
                                    egui::Rect::from_min_size(
                                        egui::pos2(br.right() - 15.0, br.top() + 1.0),
                                        egui::vec2(14.0, 13.0),
                                    )
                                });
                                // Hide the original while it's being moved (ghost shows).
                                if moving_prev != Some((t, bi)) {
                                    let selected = self.tl_sel == Some((t, bi));
                                    p.rect_filled(
                                        br,
                                        2.0,
                                        AMBER.gamma_multiply(if selected { 0.55 } else { 0.3 }),
                                    );
                                    p.rect_stroke(
                                        br,
                                        2.0,
                                        egui::Stroke::new(if selected { 2.0 } else { 1.0 }, AMBER),
                                    );
                                    p.text(
                                        br.left_top() + egui::vec2(4.0, 3.0),
                                        egui::Align2::LEFT_TOP,
                                        &block.label,
                                        egui::FontId::proportional(11.0),
                                        INK,
                                    );
                                    if let Some(bd) = badge {
                                        p.rect_filled(bd, 2.0, GROUND);
                                        p.rect_stroke(bd, 2.0, egui::Stroke::new(1.0, GREEN));
                                        p.text(
                                            bd.center(),
                                            egui::Align2::CENTER_CENTER,
                                            phase_tag(block.phase),
                                            egui::FontId::proportional(9.0),
                                            GREEN,
                                        );
                                    }
                                }
                                let bresp = ui
                                    .interact(
                                        br,
                                        egui::Id::new(("blk", t, bi)),
                                        egui::Sense::click_and_drag(),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::Grab)
                                    .on_hover_text(format!(
                                        "{} · phase: {} (click the corner badge to cycle)",
                                        block.label,
                                        phase_name(block.phase)
                                    ));
                                if bresp.clicked() {
                                    // Clicking the badge cycles the phase; elsewhere selects.
                                    let on_badge = badge
                                        .zip(bresp.interact_pointer_pos())
                                        .map(|(bd, pp)| bd.contains(pp))
                                        .unwrap_or(false);
                                    if on_badge {
                                        cycle_phase = Some((t, bi));
                                    } else {
                                        clicked = Some((t, bi));
                                    }
                                }
                                if bresp.drag_started() {
                                    if let Some(pp) = ui.input(|i| i.pointer.interact_pos()) {
                                        grab_dx = pp.x - br.left(); // keep the grab point
                                    }
                                }
                                if bresp.dragged() {
                                    if let Some(pp) = ui.input(|i| i.pointer.interact_pos()) {
                                        moving = Some((t, bi, pp));
                                        clicked = Some((t, bi));
                                    }
                                }
                                if bresp.drag_stopped() {
                                    if let Some(pp) = ui.input(|i| i.pointer.interact_pos()) {
                                        move_commit = Some((t, bi, pp));
                                    }
                                }
                            }
                            // Geometric NEW-block drop (a block-move sets no payload).
                            let dragging =
                                egui::DragAndDrop::has_payload_of_type::<DragPad>(ui.ctx())
                                    || egui::DragAndDrop::has_payload_of_type::<DragTrack>(
                                        ui.ctx(),
                                    );
                            let pointer = ui.input(|i| i.pointer.interact_pos());
                            let over = matches!(pointer, Some(pp) if lane.contains(pp));
                            if dragging {
                                if over {
                                    // Prominent: fill + thick border on the target lane.
                                    p.rect_filled(lane, 3.0, GREEN.gamma_multiply(0.22));
                                    p.rect_stroke(lane, 3.0, egui::Stroke::new(2.5, GREEN));
                                    // Preview the real block shape, beat-snapped,
                                    // near where it'll land (the exact onset snap
                                    // happens on drop, once the onset is known).
                                    if let (Some(pp), Some(len)) = (pointer, drag_preview_len) {
                                        let raw = frame_at(lane.left(), pp.x);
                                        let s = match master_bpm {
                                            Some(b) if b > 0.0 => {
                                                let fpb = (sr * 60.0 / b) as f64;
                                                ((raw as f64 / fpb).round() * fpb) as u64
                                            }
                                            _ => raw,
                                        };
                                        let x0 = x_of(lane.left(), s);
                                        let x1 = x_of(lane.left(), s + len);
                                        let pr = egui::Rect::from_min_max(
                                            egui::pos2(x0, lane.top() + 2.0),
                                            egui::pos2(x1.max(x0 + 3.0), lane.bottom() - 2.0),
                                        );
                                        p.rect_filled(pr, 2.0, GREEN.gamma_multiply(0.5));
                                        p.rect_stroke(pr, 2.0, egui::Stroke::new(1.5, GREEN));
                                    }
                                } else {
                                    p.rect_stroke(
                                        lane,
                                        3.0,
                                        egui::Stroke::new(1.0, GREEN.gamma_multiply(0.45)),
                                    );
                                }
                            }
                            if over && ui.input(|i| i.pointer.any_released()) {
                                let pp = pointer.unwrap();
                                // Pass the RAW drop frame; the onset/phase snap is
                                // applied when the onset is known (drain / inline).
                                let raw = frame_at(lane.left(), pp.x);
                                if let Some(pd) =
                                    egui::DragAndDrop::take_payload::<DragPad>(ui.ctx())
                                {
                                    drop_pad = Some((t, raw, pd.0));
                                } else if let Some(d) =
                                    egui::DragAndDrop::take_payload::<DragTrack>(ui.ctx())
                                {
                                    drop = Some((t, raw, d.0.clone()));
                                }
                            }
                            // playhead
                            let px = x_of(lane.left(), self.tl_playhead);
                            if px >= lane.left() && px <= lane.right() {
                                p.line_segment(
                                    [egui::pos2(px, lane.top()), egui::pos2(px, lane.bottom())],
                                    egui::Stroke::new(1.5, GREEN),
                                );
                            }
                            ui.add_space(4.0);
                        }
                    });
                // --- horizontal scrollbar --- only when the content exceeds the
                // view (extent includes the headroom, so the thumb reaches the end).
                if scrollable {
                    let total = extent;
                    let (bar, bresp) = ui.allocate_exact_size(
                        egui::vec2(width, 12.0),
                        egui::Sense::click_and_drag(),
                    );
                    let bp = ui.painter_at(bar);
                    bp.rect_filled(bar, 3.0, PANEL);
                    bp.rect_stroke(bar, 3.0, egui::Stroke::new(1.0, LINE));
                    let track_w = bar.width();
                    let thumb_x = bar.left() + (self.tl_scroll as f32 / total as f32) * track_w;
                    let thumb_w =
                        ((view_frames as f32 / total as f32) * track_w).clamp(24.0, track_w);
                    let thumb = egui::Rect::from_min_size(
                        egui::pos2(thumb_x.min(bar.right() - thumb_w), bar.top() + 1.0),
                        egui::vec2(thumb_w, bar.height() - 2.0),
                    );
                    bp.rect_filled(thumb, 3.0, AMBER.gamma_multiply(0.7));
                    if bresp.clicked() || bresp.dragged() {
                        if let Some(pp) = bresp.interact_pointer_pos() {
                            // Center the thumb on the pointer.
                            let rel =
                                ((pp.x - bar.left() - thumb_w / 2.0) / track_w).clamp(0.0, 1.0);
                            self.tl_scroll = ((rel * total as f32) as u64).min(max_scroll);
                        }
                    }
                }

                // Ghost following the cursor while dragging a CLIP from the grid.
                if let Some(pd) = egui::DragAndDrop::payload::<DragPad>(ui.ctx()) {
                    if let Some(pp) = ui.input(|i| i.pointer.interact_pos()) {
                        let name = self
                            .pad_source
                            .get(pd.0)
                            .and_then(|o| o.as_ref())
                            .and_then(|p| p.file_stem())
                            .and_then(|s| s.to_str())
                            .unwrap_or("clip")
                            .to_string();
                        let g = egui::Rect::from_min_size(
                            pp + egui::vec2(10.0, 6.0),
                            egui::vec2(130.0, 20.0),
                        );
                        let gp = ui.ctx().layer_painter(egui::LayerId::new(
                            egui::Order::Tooltip,
                            egui::Id::new("tl-ghost"),
                        ));
                        gp.rect_filled(g, 3.0, AMBER.gamma_multiply(0.9));
                        gp.text(
                            g.left_center() + egui::vec2(6.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            name,
                            egui::FontId::proportional(11.0),
                            GROUND,
                        );
                    }
                }
                // Ghost while moving an existing block — anchored at the grab point.
                if let Some((ft, idx, pp)) = moving {
                    if let Some(b) = self
                        .arrangement
                        .tracks()
                        .get(ft)
                        .and_then(|tr| tr.blocks.get(idx))
                    {
                        let w = ((b.out_frames(target_bpm) as f32 / sr) * pxps).max(8.0);
                        let g = egui::Rect::from_min_size(
                            egui::pos2(pp.x - grab_dx, pp.y - 12.0),
                            egui::vec2(w, 24.0),
                        );
                        let gp = ui.ctx().layer_painter(egui::LayerId::new(
                            egui::Order::Tooltip,
                            egui::Id::new("tl-move"),
                        ));
                        gp.rect_filled(g, 2.0, AMBER.gamma_multiply(0.55));
                        gp.rect_stroke(g, 2.0, egui::Stroke::new(1.5, AMBER));
                        gp.text(
                            g.left_top() + egui::vec2(4.0, 3.0),
                            egui::Align2::LEFT_TOP,
                            &b.label,
                            egui::FontId::proportional(11.0),
                            GROUND,
                        );
                    }
                }

                // Persist drag/move state for next frame.
                self.tl_grab_dx = grab_dx;
                self.tl_moving = moving.map(|(t, bi, _)| (t, bi));

                // Apply gathered actions (arrangement no longer borrowed).
                if let Some(s) = clicked {
                    self.tl_sel = Some(s);
                }
                if let Some((t, bi)) = cycle_phase {
                    // Cycle the phase + re-snap, keeping the onset near where it is.
                    if let Some(b) = self
                        .arrangement
                        .tracks()
                        .get(t)
                        .and_then(|tr| tr.blocks.get(bi))
                    {
                        let next = next_phase(b.phase);
                        let oo = self.onset_out(b);
                        let raw = b.start as f64;
                        let ns = self.snapped_start(raw, oo, next);
                        if let Some(bm) = self.arrangement.block_mut(t, bi) {
                            bm.phase = next;
                            bm.start = ns;
                        }
                    }
                }
                if let Some((ft, idx, pp)) = move_commit {
                    if let Some(nt) = lane_rects
                        .iter()
                        .position(|r| pp.y >= r.top() && pp.y <= r.bottom())
                    {
                        // Re-snap by the block's own onset + phase (cached — no
                        // recalc), anchored at the grabbed point.
                        if let Some(b) = self.arrangement.tracks()[ft].blocks.get(idx) {
                            let (oo, phase) = (self.onset_out(b), b.phase);
                            let raw = frame_at(lane_rects[nt].left(), pp.x - grab_dx) as f64;
                            let nstart = self.snapped_start(raw, oo, phase);
                            self.arrangement.move_block(ft, idx, nt, nstart);
                        }
                    }
                    self.tl_sel = None;
                    self.tl_moving = None;
                }
                if let Some((t, raw, path)) = drop {
                    self.spawn_load(
                        Target::Timeline {
                            track: t,
                            start: raw,
                        },
                        path,
                    );
                }
                if let Some((t, raw, pad)) = drop_pad {
                    let gain = self.mixer.pad_gain(pad);
                    let samples: Vec<f32> = self
                        .mixer
                        .pad_clip_region(pad)
                        .iter()
                        .map(|s| s * gain)
                        .collect();
                    if !samples.is_empty() {
                        let bpm = self.mixer.pad_bpm(pad);
                        // MASTER first, so the grid exists for the phase snap.
                        if t == 0 {
                            if let Some(b) = bpm {
                                self.mixer.set_master_bpm(Some(b));
                                self.arrangement.set_target_bpm(Some(b));
                            }
                        }
                        let label = self.pad_source[pad]
                            .as_ref()
                            .and_then(|p| p.file_stem())
                            .and_then(|s| s.to_str())
                            .unwrap_or("clip")
                            .to_string();
                        // Onset detected inline (the clip's already decoded).
                        let onset = detect_pivot(&samples, 2) as u64;
                        let mut block = Block {
                            samples: std::sync::Arc::new(samples),
                            start: 0,
                            label,
                            source_pad: Some(pad),
                            bpm,
                            onset,
                            phase: Phase::OnBeat,
                        };
                        let oo = self.onset_out(&block);
                        block.start = self.snapped_start(raw as f64, oo, block.phase);
                        self.arrangement.add_block(t, block);
                    }
                }
            });

        // Keyboard: Delete removes the selected block; Cmd/Ctrl-C/V copy/paste.
        let (del, copy, paste) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                i.modifiers.command && i.key_pressed(egui::Key::C),
                i.modifiers.command && i.key_pressed(egui::Key::V),
            )
        });
        if let Some((t, bi)) = self.tl_sel {
            if del {
                self.arrangement.remove_block(t, bi);
                self.tl_sel = None;
            } else if copy {
                self.tl_clip = self.arrangement.tracks()[t].blocks.get(bi).cloned();
            }
        }
        if paste {
            if let Some(mut b) = self.tl_clip.clone() {
                // Paste at the playhead on track 0 (or the selected track).
                let t = self.tl_sel.map(|(t, _)| t).unwrap_or(0);
                b.start = self.tl_playhead;
                self.arrangement.add_block(t, b);
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
                    if self.mixer.has_jog() && icon_btn(ui, ph::X, "clear the platter") {
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
            .unwrap_or("clip")
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

    /// Re-flow a clip's current trimmed region (× its gain) into every timeline
    /// block placed from that pad — keeps placed blocks in sync after a trim or
    /// volume edit on the clip.
    fn refresh_pad_blocks(&mut self, pad: usize) {
        let gain = self.mixer.pad_gain(pad);
        let samples: Vec<f32> = self
            .mixer
            .pad_clip_region(pad)
            .iter()
            .map(|s| s * gain)
            .collect();
        self.arrangement
            .refresh_pad(pad, std::sync::Arc::new(samples));
    }

    /// Place a block so its onset lands on the phase target nearest `raw_start`
    /// (timeline frames). `onset_out` is the onset offset in *timeline* frames
    /// (source onset ÷ varispeed). With no master tempo, returns `raw_start`.
    fn snapped_start(&self, raw_start: f64, onset_out: f64, phase: Phase) -> u64 {
        let sr = self.target_rate.max(1) as f64;
        let fpb = match self.mixer.master_bpm() {
            Some(b) if b > 0.0 => sr * 60.0 / b as f64,
            _ => return raw_start.max(0.0) as u64,
        };
        match phase {
            // No onset fix — the clip's file start sits on the beat.
            Phase::Free => ((raw_start / fpb).round() * fpb).max(0.0) as u64,
            _ => {
                let onset_raw = raw_start + onset_out; // where the hit is now
                let grid = if phase == Phase::Bar { fpb * 4.0 } else { fpb };
                let off = if phase == Phase::OffBeat {
                    fpb / 2.0
                } else {
                    0.0
                };
                let anchor = ((onset_raw - off) / grid).round() * grid + off;
                (anchor - onset_out).max(0.0) as u64
            }
        }
    }

    /// The onset offset of a block in timeline frames (source onset ÷ varispeed).
    fn onset_out(&self, b: &Block) -> f64 {
        b.onset as f64 / b.speed(self.arrangement.target_bpm())
    }

    /// Render the whole timeline arrangement to a `mix-N.wav` in the library.
    fn render_arrangement(&mut self) {
        let samples = self.arrangement.render();
        if samples.is_empty() {
            return;
        }
        let dir = self.crate_lib.cwd().to_path_buf();
        let mut n = 1;
        let path = loop {
            let p = dir.join(format!("mix-{n}.wav"));
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        if let Err(e) = write_wav(&path, &samples, self.mixer.sample_rate(), 2) {
            tracing::error!(error = %e, "render failed");
            return;
        }
        self.crate_lib.refresh();
        self.probed_dir = None;
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

        // Top + bottom panels first (they reserve their edges); both borrow
        // &mut self directly.
        self.draw_timeline(ctx);
        self.draw_scratch_panel(ctx);

        let mut acts: Vec<Act> = Vec::new();
        let mut spring_hover: Option<PathBuf> = None;
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
            draw_library(
                ctx,
                crate_lib,
                lib_sel,
                renaming,
                new_folder,
                playable,
                previewing.as_deref(),
                &mut spring_hover,
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

        // Spring-loaded folders: hold a dragged track over a folder / "up" and
        // we navigate into it after ~0.5s, so you can drop into another folder.
        const SPRING_SECS: f64 = 0.5;
        let now = ctx.input(|i| i.time);
        match (&self.spring, &spring_hover) {
            (_, None) => self.spring = None,
            (Some((p, t0)), Some(h)) if p == h => {
                if now - t0 > SPRING_SECS {
                    self.crate_lib.enter(h);
                    self.lib_sel = None;
                    self.spring = None;
                }
            }
            (_, Some(h)) => self.spring = Some((h.clone(), now)),
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
    // Body text stays in the Proportional family — Space Mono is its first font
    // (so it reads monospace) AND the Phosphor icon font is registered there, so
    // icons resolve. Forcing Monospace here hid the icons (tofu squares).
    style.interaction.selectable_labels = false;
    ctx.set_style(style);
}

/// A prominent painted close button (an amber X). Returns true on click. Used
/// everywhere we close a view, instead of a "done" word.
fn close_x(ui: &mut egui::Ui) -> bool {
    ui.add(egui::Button::new(egui::RichText::new(ph::X).size(20.0)).frame(false))
        .on_hover_cursor(egui::CursorIcon::Default)
        .on_hover_text("close")
        .clicked()
}

/// Small painted "new folder" button (a folder tab + a +). Returns true on click.
fn folder_plus_button(ui: &mut egui::Ui) -> bool {
    icon_btn(ui, ph::FOLDER_PLUS, "new folder")
}

/// Trash icon that is a drop target (drag a track here to delete) and clickable
/// (delete the selection). Reddens while a track is being dragged.
fn trash_zone(ui: &mut egui::Ui) -> (bool, Option<std::sync::Arc<DragTrack>>) {
    let dragging = egui::DragAndDrop::has_payload_of_type::<DragTrack>(ui.ctx());
    let col = if dragging { RED } else { DIM };
    let resp = ui
        .add(egui::Button::new(egui::RichText::new(ph::TRASH).size(18.0).color(col)).frame(false))
        .on_hover_cursor(egui::CursorIcon::Default)
        .on_hover_text("drag here / click to delete");
    let dropped = resp.dnd_release_payload::<DragTrack>();
    (resp.clicked(), dropped)
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
    // Phosphor icon font, appended as a fallback so icon glyphs render.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

/// The next phase in the cycle (the per-block badge clicks through these).
fn next_phase(p: Phase) -> Phase {
    match p {
        Phase::OnBeat => Phase::Bar,
        Phase::Bar => Phase::OffBeat,
        Phase::OffBeat => Phase::Free,
        Phase::Free => Phase::OnBeat,
    }
}

/// Short badge tag drawn on a block.
fn phase_tag(p: Phase) -> &'static str {
    match p {
        Phase::OnBeat => "B",
        Phase::Bar => "R",
        Phase::OffBeat => "O",
        Phase::Free => "F",
    }
}

/// Full phase name (tooltips).
fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::OnBeat => "on-beat",
        Phase::Bar => "bar",
        Phase::OffBeat => "off-beat",
        Phase::Free => "free",
    }
}

/// A clean icon-only button (no frame). Returns true on click.
fn icon_btn(ui: &mut egui::Ui, icon: &str, tip: &str) -> bool {
    ui.add(egui::Button::new(egui::RichText::new(icon).size(17.0)).frame(false))
        .on_hover_cursor(egui::CursorIcon::Default)
        .on_hover_text(tip)
        .clicked()
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

#[allow(clippy::too_many_arguments)]
fn draw_library(
    ctx: &egui::Context,
    lib: &Crate,
    sel: &Option<PathBuf>,
    renaming: &mut Option<(PathBuf, String)>,
    new_folder: &mut Option<String>,
    playable: &HashMap<PathBuf, bool>,
    previewing: Option<&Path>,
    spring_hover: &mut Option<PathBuf>,
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
                        draw_folder_row(ui, ".. (up)", parent, spring_hover, acts);
                    }
                }
                for e in lib.entries() {
                    if e.name == ".." {
                        continue; // handled by the explicit up button
                    }
                    if e.is_dir {
                        draw_folder_row(ui, &e.name, &e.path, spring_hover, acts);
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
fn draw_folder_row(
    ui: &mut egui::Ui,
    name: &str,
    path: &Path,
    spring_hover: &mut Option<PathBuf>,
    acts: &mut Vec<Act>,
) {
    let is_up = name.ends_with(')'); // ".. (up)"
    let label = if is_up {
        format!("{}  {name}", ph::ARROW_UP)
    } else {
        format!("{}  {name}/", ph::FOLDER)
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
        // Spring-load: tell the timer a drag is hovering this folder.
        *spring_hover = Some(path.to_path_buf());
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
        // Per-row play/stop button (preview).
        let glyph = if playing { ph::STOP } else { ph::PLAY };
        let btn = egui::Button::new(egui::RichText::new(glyph).size(15.0).color(if playing {
            AMBER
        } else {
            INK
        }))
        .frame(false);
        if ui
            .add_enabled(!bad, btn)
            .on_hover_cursor(egui::CursorIcon::Default)
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
            ui.label(bungee("clips", 14.0, AMBER));
            ui.label(egui::RichText::new("drag a track onto a clip to load").color(DIM));
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
                ui.add_space(8.0); // a little breathing room between pad rows
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
            if icon_btn(
                ui,
                if playing { ph::STOP } else { ph::PLAY },
                "play / stop selection",
            ) {
                acts.push(Act::AuditionSel(i));
            }
            if icon_btn(ui, ph::FLOPPY_DISK, "export to library (WAV)") {
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
                "in {:.3}s   out {:.3}s   selection {:.3}s   ·  drag the handles to trim",
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
            let resp = ui
                .interact(handle, id, egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            let painter = ui.painter_at(rect);
            // The handle: a full-height line + a grab knob at the top.
            painter.line_segment(
                [egui::pos2(hx, rect.top()), egui::pos2(hx, rect.bottom())],
                egui::Stroke::new(2.0, AMBER),
            );
            let knob = egui::Rect::from_center_size(
                egui::pos2(hx, rect.top() + 7.0),
                egui::vec2(10.0, 12.0),
            );
            painter.rect_filled(knob, 3.0, AMBER);
            painter.rect_filled(
                egui::Rect::from_center_size(knob.center(), egui::vec2(2.0, 6.0)),
                0.0,
                GROUND,
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
    // A plain group frame (NOT a dnd drop-zone — a drop-zone swallows the inner
    // drag-handle's drag). We detect the library-track drop on its response.
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width()); // fill the equal column → uniform cells
        ui.set_min_height(96.0);
        ui.vertical(|ui| {
            // Header: play/pause + the (truncated) track name. No pad number.
            ui.horizontal(|ui| {
                if loaded {
                    let icon = if mixer.pad_is_sounding(i) {
                        ph::PAUSE
                    } else {
                        ph::PLAY
                    };
                    if icon_btn(ui, icon, "play / pause") {
                        acts.push(Act::PlayPad(i));
                    }
                }
                // Drag handle: only the grip + name strip is the drag source, so
                // the kind/volume/edit/clear/export controls below keep their clicks.
                let handle = ui
                    .horizontal(|ui| {
                        if loaded {
                            ui.label(egui::RichText::new(ph::DOTS_SIX_VERTICAL).color(DIM));
                        }
                        let name = if track.is_empty() { "—" } else { &track };
                        ui.add(
                            egui::Label::new(egui::RichText::new(name).color(AMBER).strong())
                                .truncate(),
                        );
                    })
                    .response;
                if loaded {
                    let handle = handle
                        .interact(egui::Sense::click_and_drag())
                        .on_hover_text("drag the clip onto the timeline");
                    handle.dnd_set_drag_payload(DragPad(i));
                    if handle.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    } else {
                        handle.on_hover_cursor(egui::CursorIcon::Grab);
                    }
                }
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

            // Actions — icon buttons (no "on" toggle; a loaded pad is live).
            ui.horizontal(|ui| {
                if icon_btn(ui, ph::PENCIL_SIMPLE, "trim the clip") {
                    acts.push(Act::EditClip(i));
                }
                if icon_btn(ui, ph::ERASER, "clear the clip") {
                    acts.push(Act::ClearPad(i));
                }
                if icon_btn(ui, ph::FLOPPY_DISK, "export to library (WAV)") {
                    acts.push(Act::ExportPad(i));
                }
            });
        });
    });
    // Cell-level HOVER interact (not drag/click — so it never eats the inner
    // buttons' clicks): just receives a dragged library track + the highlight.
    // The drag source is the header handle above.
    let cell = ui.interact(
        inner.response.rect,
        egui::Id::new(("clipcell", i)),
        egui::Sense::hover(),
    );
    if cell.dnd_hover_payload::<DragTrack>().is_some() {
        let r = inner.response.rect;
        ui.painter().rect_filled(r, 4.0, AMBER.gamma_multiply(0.15));
        ui.painter()
            .rect_stroke(r, 4.0, egui::Stroke::new(2.0, AMBER));
    }
    if let Some(p) = cell.dnd_release_payload::<DragTrack>() {
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
