//! The egui/eframe desktop front-end — the sole UI (see the 2026-06-08 GUI
//! pivot in `.am/inception.md`). The engine (`termkrush-core`) is unchanged;
//! this is all view + input.
//!
//! Post the 2026-06-11 auto-mix pivot this shell is three surfaces: the
//! library (left), the beat-tap clip editor (central, opened from a library
//! row), and the sequence line (bottom) — the product's only arranging
//! surface, autosaved as the project file. Pads, the timeline, and the
//! scratch platter are gone — the engine performs the mix; the user curates
//! tracks, taps beats, and orders the sequence.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

use eframe::egui;
use egui_phosphor::regular as ph;
use termkrush_core::audio::{
    decode_file, detect_bpm, probe_playable, write_wav, AudioOutput, DecodedAudio,
};
use termkrush_core::automix;
use termkrush_core::beats::{beats_path, fit_grid, BeatCache};
use termkrush_core::config::Config;
use termkrush_core::library::Crate;
use termkrush_core::mix::{Mixer, PADS};
use termkrush_core::sequence::{sequence_path, Sequence};

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

/// The mixer voice that backs the clip editor. The pad grid is gone, but the
/// engine's pad voices remain — the editor borrows one as its audition slot.
const EDIT_SLOT: usize = 0;

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

/// A track being dragged out of the library (drop onto a folder to move it,
/// or onto the sequence line to add it).
#[derive(Clone)]
struct DragTrack(PathBuf);

/// A sequence entry being dragged to a new position (by its current index).
#[derive(Clone)]
struct DragEntry(usize);

/// Where a background decode is headed when it lands.
#[derive(Clone, Copy)]
enum Target {
    /// The clip editor's audition slot.
    Pad(usize),
    Preview,
}

/// Where a background render is, phase-wise — drives the central render
/// panel and the sequence line's progress strip (see the big-poppa design).
#[derive(Clone, Copy, PartialEq, Eq)]
enum RenderPhase {
    Decode,
    GridFit,
    Arrange,
    Bounce,
}

impl RenderPhase {
    fn label(self) -> &'static str {
        match self {
            RenderPhase::Decode => "decode",
            RenderPhase::GridFit => "grid fit",
            RenderPhase::Arrange => "arrange + scratch",
            RenderPhase::Bounce => "bounce WAV",
        }
    }
    fn all() -> [RenderPhase; 4] {
        [
            RenderPhase::Decode,
            RenderPhase::GridFit,
            RenderPhase::Arrange,
            RenderPhase::Bounce,
        ]
    }
}

/// The facts the render-complete panel shows (and the proof line: same
/// sequence → same seed → same mix).
#[derive(Clone)]
struct MixDone {
    path: PathBuf,
    secs: f64,
    master_bpm: f64,
    sections: usize,
    seed: u64,
}

/// Messages from the background render worker.
enum RenderMsg {
    Progress(RenderPhase, f32),
    Done(Result<MixDone, String>),
    Mp3Done(Result<PathBuf, String>),
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
    MoveTo {
        track: PathBuf,
        folder: PathBuf,
    },
    StartNewFolder,
    CommitNewFolder,
    CancelNewFolder,
    /// Open a library track in the beat-tap clip editor.
    EditTrack(PathBuf),
    /// Insert a track into the sequence so it plays at position `idx`.
    SeqInsert {
        idx: usize,
        path: PathBuf,
    },
    /// Remove the sequence entry at this position.
    SeqRemove(usize),
    /// Move a sequence entry from one position to another.
    SeqMove {
        from: usize,
        to: usize,
    },
    CloseClip,
    SetTrimIn(usize, usize),
    SetTrimOut(usize, usize),
    AuditionSel(usize),
    ExportClip(usize),
    /// Clear all beat marks on the clip.
    ClearBeats(usize),
    /// Toggle a beat mark at a clip-absolute frame (add, or remove if near one).
    ToggleBeat(usize, u64),
    /// Tap: add a beat mark at the clip's current play position (add-only).
    TapBeat(usize),
    /// Render the sequence to a mix WAV via the auto-mix engine.
    RenderMix,
    /// Export the finished mix as MP3 (next to the WAV, in the library).
    ExportMp3(PathBuf),
    /// Dismiss the render-complete panel.
    DismissMix,
}

/// The whole app: the engine, the audio sink, and the browsed library.
pub struct TermKrushApp {
    mixer: Mixer,
    crate_lib: Crate,
    /// Source path loaded on each engine voice (the editor uses `EDIT_SLOT`).
    pad_source: [Option<PathBuf>; PADS],
    /// Beat marks per voice, in clip-absolute frames (set in the clip editor).
    pad_beats: [Vec<u64>; PADS],
    /// The selected library track (delete / preview act on it).
    lib_sel: Option<PathBuf>,
    /// Inline rename in progress: `(target, buffer)`.
    renaming: Option<(PathBuf, String)>,
    /// Inline new-folder name being typed.
    new_folder: Option<String>,
    /// The library track currently previewing (so its row shows a stop button).
    previewing: Option<PathBuf>,
    /// Whether the preview has actually started sounding (to clear `previewing`
    /// only after it finishes, not during the decode gap).
    preview_was_on: bool,
    /// The voice whose clip is open in the editor (central-panel mode).
    clip_edit: Option<usize>,
    /// True while a decode headed for the editor is in flight — the editor
    /// opens when it lands.
    edit_pending: bool,
    /// Cached clip-editor waveform `(slot, peaks)` — computed once on open so the
    /// whole-clip downsample doesn't run every frame (which starved the audio).
    clip_wave: Option<(usize, Vec<(f32, f32)>)>,
    /// How many background decodes are in flight (drives the loading overlay).
    pending_decodes: usize,
    /// Spring-loaded folders: `(folder, hover-start time)` while a track is
    /// dragged over it — after a hold we navigate in so you can drop elsewhere.
    spring: Option<(PathBuf, f64)>,
    /// The ordered track sequence — the project. Autosaved on every change.
    sequence: Sequence,
    /// Where the sequence autosaves (`None` when the home dir is unknown —
    /// the sequence still works for the session, it just can't persist).
    seq_path: Option<PathBuf>,
    /// Every track's tapped beats — tap once, ever. Autosaved like the
    /// sequence; marks follow renames/moves and die with deletes.
    beats: BeatCache,
    /// Where the beat cache autosaves (`None` when the home dir is unknown).
    beats_file: Option<PathBuf>,
    /// Live phase/percent while a background auto-mix render runs.
    render_progress: Option<(RenderPhase, f32)>,
    /// The last finished mix — the central panel celebrates it until the
    /// user moves on (opens the editor / starts another render).
    last_mix: Option<MixDone>,
    render_tx: Sender<RenderMsg>,
    render_rx: Receiver<RenderMsg>,

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
        let (render_tx, render_rx) = channel();

        // Reopening the app restores the last sequence — the project file —
        // and every track's tapped beats.
        let seq_path = sequence_path();
        let sequence = seq_path.as_deref().map(Sequence::load).unwrap_or_default();
        let beats_file = beats_path();
        let beats = beats_file
            .as_deref()
            .map(BeatCache::load)
            .unwrap_or_default();
        tracing::info!(entries = sequence.len(), "sequence restored");

        Self {
            mixer,
            crate_lib,
            pad_source: Default::default(),
            pad_beats: Default::default(),
            lib_sel: None,
            renaming: None,
            new_folder: None,
            previewing: None,
            preview_was_on: false,
            clip_edit: None,
            edit_pending: false,
            clip_wave: None,
            pending_decodes: 0,
            spring: None,
            sequence,
            seq_path,
            beats,
            beats_file,
            render_progress: None,
            last_mix: None,
            render_tx,
            render_rx,
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
                    // A rough detected tempo for the editor's voice; the tapped
                    // beats (least-squares fit) are the exact source of truth.
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
                self.edit_pending = false; // failed decode never opens the editor
                continue;
            };
            match done.target {
                Target::Pad(i) => {
                    self.mixer.assign_pad(i, audio.samples);
                    if let Some(b) = done.bpm {
                        self.mixer.set_pad_bpm(i, Some(b));
                    }
                    // A previously-tapped track opens with its cached marks
                    // (rescaled if this device runs at a different rate).
                    let cached = self
                        .beats
                        .get(&done.path)
                        .map(|m| m.at_rate(self.target_rate))
                        .unwrap_or_default();
                    self.pad_source[i] = Some(done.path);
                    if self.edit_pending && i == EDIT_SLOT {
                        self.edit_pending = false;
                        self.pad_beats[i] = cached;
                        self.clip_edit = Some(i);
                        // Downsample the whole clip ONCE; reused every frame.
                        self.clip_wave = Some((i, self.mixer.pad_peaks(i, WAVE_COLS)));
                    }
                }
                Target::Preview => self.mixer.preview(audio.samples),
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
                    if !buf.is_empty() && self.crate_lib.rename(&p, buf).is_ok() {
                        // Keep sequence entries + beat marks pointing at the
                        // renamed file.
                        let new = p.parent().unwrap_or(Path::new("")).join(buf);
                        self.sequence.retarget(&p, &new);
                        self.save_sequence();
                        self.beats.retarget(&p, &new);
                        self.save_beats();
                    }
                }
            }
            Act::CancelRename => self.renaming = None,
            Act::Delete(p) => {
                if self.crate_lib.delete(&p).is_ok() {
                    // A deleted track can't play — drop its sequence entries
                    // and its beat marks.
                    self.sequence.purge(&p);
                    self.save_sequence();
                    self.beats.purge(&p);
                    self.save_beats();
                }
                if self.lib_sel.as_deref() == Some(p.as_path()) {
                    self.lib_sel = None;
                }
            }
            Act::MoveTo { track, folder } => {
                if self.crate_lib.move_into(&track, &folder).is_ok() {
                    // Keep sequence entries + beat marks pointing at the
                    // moved file.
                    if let Some(name) = track.file_name() {
                        let new = folder.join(name);
                        self.sequence.retarget(&track, &new);
                        self.save_sequence();
                        self.beats.retarget(&track, &new);
                        self.save_beats();
                    }
                }
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
            Act::EditTrack(path) => {
                // Opening the editor silences any preview, then decodes into
                // the audition slot; the editor opens when the decode lands.
                self.mixer.stop_preview();
                self.previewing = None;
                self.mixer.stop_pad(EDIT_SLOT);
                self.edit_pending = true;
                self.spawn_load(Target::Pad(EDIT_SLOT), path);
            }
            Act::SeqInsert { idx, path } => {
                self.sequence.insert(idx, path);
                self.save_sequence();
            }
            Act::SeqRemove(idx) => {
                self.sequence.remove(idx);
                self.save_sequence();
            }
            Act::SeqMove { from, to } => {
                self.sequence.move_entry(from, to);
                self.save_sequence();
            }
            Act::CloseClip => {
                if let Some(i) = self.clip_edit.take() {
                    self.mixer.stop_pad(i);
                    // "save" persists the taps — the track is marked for good.
                    if let Some(track) = self.pad_source[i].clone() {
                        self.beats
                            .set(&track, self.target_rate, self.pad_beats[i].clone());
                        self.save_beats();
                    }
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
            Act::ExportClip(i) => self.export_clip(i),
            Act::ClearBeats(i) => self.pad_beats[i].clear(),
            Act::ToggleBeat(i, frame) => {
                // Remove a nearby mark (within ~30ms), else add one.
                let tol = (self.target_rate as u64 * 30) / 1000;
                if let Some(pos) = self.pad_beats[i]
                    .iter()
                    .position(|&b| b.abs_diff(frame) <= tol)
                {
                    self.pad_beats[i].remove(pos);
                } else {
                    self.pad_beats[i].push(frame);
                    self.pad_beats[i].sort_unstable();
                }
            }
            Act::TapBeat(i) => {
                // Add a mark at the live play position (add-only, ~10ms dup guard).
                if let Some(f) = self.mixer.pad_play_pos(i) {
                    let f = f as u64;
                    let dup = (self.target_rate as u64 * 10) / 1000;
                    if !self.pad_beats[i].iter().any(|&b| b.abs_diff(f) <= dup) {
                        self.pad_beats[i].push(f);
                        self.pad_beats[i].sort_unstable();
                    }
                }
            }
            Act::RenderMix => self.start_render(),
            Act::ExportMp3(wav) => {
                let tx = self.render_tx.clone();
                std::thread::spawn(move || {
                    let res = (|| {
                        let audio =
                            decode_file(&wav, automix::RENDER_RATE).map_err(|e| e.to_string())?;
                        let out = wav.with_extension("mp3");
                        termkrush_core::audio::export_mp3(
                            &out,
                            &audio.samples,
                            automix::RENDER_RATE,
                            2,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(out)
                    })();
                    let _ = tx.send(RenderMsg::Mp3Done(res));
                });
            }
            Act::DismissMix => self.last_mix = None,
        }
    }

    /// Kick off the auto-mix render on a background thread: decode every
    /// unique sequence track at the fixed render rate, feed the cached
    /// marks, plan + render deterministically, write `automix-N.wav` into
    /// the current library folder.
    fn start_render(&mut self) {
        if self.render_progress.is_some() || self.sequence.is_empty() {
            return;
        }
        self.render_progress = Some((RenderPhase::Decode, 0.0));
        self.last_mix = None;
        let entries: Vec<PathBuf> = self.sequence.entries().to_vec();
        let beats = self.beats.clone();
        let out_dir = self.crate_lib.cwd().to_path_buf();
        let tx = self.render_tx.clone();
        std::thread::spawn(move || {
            let done = render_sequence(&entries, &beats, &out_dir, &tx);
            let _ = tx.send(RenderMsg::Done(done));
        });
    }

    /// Master tempo readout: the first sequence entry's fitted tempo (the
    /// grid the whole mix locks to) plus the track that set it.
    fn master_info(&self) -> Option<(f32, String)> {
        let first = self.sequence.entries().first()?;
        let m = self.beats.get(first)?;
        let bpm = Self::bpm_from_beats(&m.frames, m.sample_rate)?;
        let name = first
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("track")
            .to_string();
        Some((bpm, name))
    }

    /// Write the editor clip's trimmed region to the current library folder as
    /// a WAV.
    fn export_clip(&mut self, i: usize) {
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

    /// Autosave the sequence (the project file). Every mutation funnels
    /// through here, so the project on disk is never behind the screen.
    fn save_sequence(&self) {
        let Some(path) = self.seq_path.as_deref() else {
            return; // no home dir: session-only sequence
        };
        if let Err(e) = self.sequence.save(path) {
            tracing::error!(error = %e, path = %path.display(), "sequence autosave failed");
        }
    }

    /// Autosave the beat cache, mirroring `save_sequence`.
    fn save_beats(&self) {
        let Some(path) = self.beats_file.as_deref() else {
            return; // no home dir: session-only marks
        };
        if let Err(e) = self.beats.save(path) {
            tracing::error!(error = %e, path = %path.display(), "beats autosave failed");
        }
    }

    /// Tempo (BPM) from a clip's tapped beats — uses the FITTED interval (the
    /// least-squares regular spacing), so imperfect taps average out. `None`
    /// with fewer than 2 marks.
    fn bpm_from_beats(beats: &[u64], sr: u32) -> Option<f32> {
        fit_grid(beats).map(|(_phase, interval)| sr as f32 * 60.0 / interval as f32)
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

        // Background render messages: live phase/percent, the finished
        // mix (refresh the library so it shows up + gets probed), or a
        // completed MP3 export.
        while let Ok(msg) = self.render_rx.try_recv() {
            match msg {
                RenderMsg::Progress(phase, pct) => {
                    self.render_progress = Some((phase, pct));
                }
                RenderMsg::Done(done) => {
                    self.render_progress = None;
                    match done {
                        Ok(mix) => {
                            tracing::info!(path = %mix.path.display(), seed = mix.seed, "mix rendered");
                            self.crate_lib.refresh();
                            self.probed_dir = None;
                            self.last_mix = Some(mix);
                        }
                        Err(e) => tracing::error!(error = %e, "render failed"),
                    }
                }
                RenderMsg::Mp3Done(res) => match res {
                    Ok(path) => {
                        tracing::info!(path = %path.display(), "mp3 exported");
                        self.crate_lib.refresh();
                        self.probed_dir = None;
                    }
                    Err(e) => tracing::error!(error = %e, "mp3 export failed"),
                },
            }
        }

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

        let master = self.master_info();
        draw_top_bar(ctx, master.as_ref(), self.crate_lib.root());

        let mut acts: Vec<Act> = Vec::new();
        let mut spring_hover: Option<PathBuf> = None;
        {
            let TermKrushApp {
                mixer,
                crate_lib,
                pad_source,
                pad_beats,
                lib_sel,
                renaming,
                new_folder,
                playable,
                clip_edit,
                clip_wave,
                previewing,
                target_rate,
                sequence,
                beats,
                render_progress,
                ..
            } = self;
            let render_progress = *render_progress;
            let working = render_progress.map(|(_, pct)| pct);
            draw_sequence_line(ctx, sequence, beats, working, master.as_ref(), &mut acts);
            draw_library(
                ctx,
                crate_lib,
                lib_sel,
                renaming,
                new_folder,
                playable,
                beats,
                previewing.as_deref(),
                &mut spring_hover,
                &mut acts,
            );
            // Central panel: the clip editor when one is open, else the (empty)
            // main area.
            if let Some(i) = *clip_edit {
                let wave = clip_wave
                    .as_ref()
                    .filter(|(p, _)| *p == i)
                    .map(|(_, w)| w.as_slice())
                    .unwrap_or(&[]);
                let name = pad_source[i]
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .and_then(|s| s.to_str())
                    .unwrap_or("clip")
                    .to_string();
                let saved = pad_source[i]
                    .as_ref()
                    .and_then(|p| beats.get(p))
                    .map(|m| m.at_rate(*target_rate) == pad_beats[i])
                    .unwrap_or(false);
                draw_clip_editor(
                    ctx,
                    mixer,
                    &name,
                    i,
                    wave,
                    &pad_beats[i],
                    *target_rate,
                    saved,
                    &mut acts,
                );
            } else if let Some((phase, pct)) = self.render_progress {
                draw_render_panel(ctx, phase, pct, master.as_ref());
            } else if let Some(mix) = &self.last_mix {
                draw_complete_panel(ctx, mix, &mut acts);
            } else {
                draw_coach(ctx);
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

/// The render worker (runs off-thread): decode each unique track at the
/// fixed engine rate, marks from the cache, plan + render, write the WAV.
/// Everything here is deterministic for a given (entries, beats) input;
/// progress messages drive the render panel and are display-only.
fn render_sequence(
    entries: &[PathBuf],
    beats: &BeatCache,
    out_dir: &Path,
    tx: &Sender<RenderMsg>,
) -> Result<MixDone, String> {
    // Unique tracks in first-appearance order; the order list indexes them.
    let mut uniq: Vec<&PathBuf> = Vec::new();
    let mut order = Vec::with_capacity(entries.len());
    for e in entries {
        let idx = match uniq.iter().position(|u| *u == e) {
            Some(i) => i,
            None => {
                uniq.push(e);
                uniq.len() - 1
            }
        };
        order.push(idx);
    }

    let mut tracks = Vec::with_capacity(uniq.len());
    for (i, path) in uniq.iter().enumerate() {
        let pct = 5.0 + 55.0 * i as f32 / uniq.len() as f32;
        let _ = tx.send(RenderMsg::Progress(RenderPhase::Decode, pct));
        let audio = decode_file(path, automix::RENDER_RATE)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let marks = beats
            .get(path)
            .map(|m| m.at_rate(automix::RENDER_RATE))
            .unwrap_or_default();
        tracks.push(automix::TrackInput {
            id: path.to_string_lossy().into_owned(),
            samples: std::sync::Arc::new(audio.samples),
            beats: marks,
        });
    }

    let _ = tx.send(RenderMsg::Progress(RenderPhase::GridFit, 64.0));
    let plan = automix::plan(&tracks, &order).map_err(|e| e.to_string())?;
    let _ = tx.send(RenderMsg::Progress(RenderPhase::Arrange, 72.0));
    let mix = automix::render(&plan, &tracks);
    let _ = tx.send(RenderMsg::Progress(RenderPhase::Bounce, 92.0));
    tracing::info!(
        master_bpm = plan.master_bpm,
        sections = plan.sections.len(),
        seed = plan.seed,
        "automix rendered"
    );

    let mut n = 1;
    let out = loop {
        let p = out_dir.join(format!("automix-{n}.wav"));
        if !p.exists() {
            break p;
        }
        n += 1;
    };
    write_wav(&out, &mix, automix::RENDER_RATE, 2).map_err(|e| e.to_string())?;
    Ok(MixDone {
        path: out,
        secs: plan.total_frames() as f64 / automix::RENDER_RATE as f64,
        master_bpm: plan.master_bpm,
        sections: plan.sections.len(),
        seed: plan.seed,
    })
}

/// The top bar: brand on the left; status on the right — the crate path
/// and the master-grid readout ("grid locked · 92 bpm master · track_01").
/// Status only: the bar carries zero knobs by design.
fn draw_top_bar(ctx: &egui::Context, master: Option<&(f32, String)>, crate_root: &Path) {
    egui::TopBottomPanel::top("brand")
        .exact_height(34.0)
        .show(ctx, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                let p = ui.painter_at(r);
                p.circle_filled(r.center(), 10.0, PANEL);
                p.circle_stroke(r.center(), 10.0, egui::Stroke::new(1.0, LINE));
                p.circle_filled(r.center(), 3.0, AMBER);
                ui.label(bungee("termkrush", 18.0, AMBER));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match master {
                        Some((bpm, from)) => {
                            ui.label(
                                egui::RichText::new(format!("{bpm:.0} bpm master · {from}"))
                                    .color(AMBER),
                            );
                            ui.label(egui::RichText::new("grid locked").color(GREEN).small());
                        }
                        None => {
                            ui.label(egui::RichText::new("— bpm master").color(DIM));
                            ui.label(egui::RichText::new("no grid").color(DIM).small());
                        }
                    }
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  crate {}",
                            ph::SQUARES_FOUR,
                            crate_root.display()
                        ))
                        .color(DIM)
                        .small(),
                    );
                });
            });
        });
}

/// First-run coach (central, when nothing else claims the stage): the one
/// idea the user must get — tap a beat, once — and the three-step path.
fn draw_coach(ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(ui.available_height() * 0.24);
        ui.vertical_centered(|ui| {
            ui.label(bungee("↓", 34.0, GREEN));
            ui.add_space(6.0);
            ui.label(bungee("tap a beat, once", 22.0, INK));
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(
                    "Pick a track in the library and hit the pencil. Play it, tap the                      ↓ arrow on each beat — termkrush fits an exact tempo and downbeat.                      You tap each track once, ever.",
                )
                .color(DIM),
            );
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let w = ui.available_width();
                ui.add_space((w - 360.0).max(0.0) / 2.0);
                for (n, label) in [(1, "tap beats"), (2, "line up tracks"), (3, "render")] {
                    ui.label(egui::RichText::new(format!("{n}")).color(GREEN).strong());
                    ui.label(egui::RichText::new(label).color(INK));
                    if n < 3 {
                        ui.add_space(18.0);
                    }
                }
            });
        });
    });
}

/// The central render panel: spinning vinyl, progress, and the phase chips
/// (decode → varispeed/grid fit → arrange + scratch → bounce WAV).
fn draw_render_panel(
    ctx: &egui::Context,
    phase: RenderPhase,
    pct: f32,
    master: Option<&(f32, String)>,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(ui.available_height() * 0.2);
        ui.vertical_centered(|ui| {
            // The vinyl: rings + amber label, spun by time while rendering.
            let (r, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
            let p = ui.painter_at(r);
            let c = r.center();
            p.circle_filled(c, 30.0, PANEL);
            for k in 1..5 {
                p.circle_stroke(c, 30.0 * k as f32 / 5.0, egui::Stroke::new(1.0, LINE));
            }
            let ang = (ui.input(|i| i.time) * 2.2) as f32;
            p.line_segment(
                [c, c + 28.0 * egui::vec2(ang.cos(), ang.sin())],
                egui::Stroke::new(2.0, AMBER),
            );
            p.circle_filled(c, 8.0, AMBER);
            ui.add_space(10.0);
            ui.label(bungee("rendering mix", 20.0, INK));
            ui.add_space(12.0);

            // Progress bar.
            let w = ui.available_width().min(460.0);
            let (bar, _) = ui.allocate_exact_size(egui::vec2(w, 10.0), egui::Sense::hover());
            let bp = ui.painter_at(bar);
            bp.rect_filled(bar, 4.0, PANEL);
            bp.rect_stroke(bar, 4.0, egui::Stroke::new(1.0, LINE));
            let mut fill = bar;
            fill.set_width(bar.width() * (pct / 100.0).clamp(0.0, 1.0));
            bp.rect_filled(fill, 4.0, AMBER.gamma_multiply(0.85));
            ui.label(
                egui::RichText::new(format!("{pct:.0}%"))
                    .color(DIM)
                    .small(),
            );
            ui.add_space(10.0);

            // Phase chips: done (green), now (amber), pending (dim).
            ui.horizontal(|ui| {
                let total: f32 = 320.0;
                ui.add_space((ui.available_width() - total).max(0.0) / 2.0);
                let phases = RenderPhase::all();
                let now = phases.iter().position(|p| *p == phase).unwrap_or(0);
                for (i, ph_) in phases.iter().enumerate() {
                    let (color, mark) = match i.cmp(&now) {
                        std::cmp::Ordering::Less => (GREEN, "✓ "),
                        std::cmp::Ordering::Equal => (AMBER, "● "),
                        std::cmp::Ordering::Greater => (DIM, ""),
                    };
                    ui.label(
                        egui::RichText::new(format!("{mark}{}", ph_.label()))
                            .color(color)
                            .small(),
                    );
                    if i < phases.len() - 1 {
                        ui.label(egui::RichText::new("›").color(LINE));
                    }
                }
            });
            ui.add_space(14.0);
            if let Some((bpm, from)) = master {
                ui.label(
                    egui::RichText::new(format!(
                        "everything varispeeds to {bpm:.0} BPM — the grid set by {from} never moves.                          seeded, so this render is bit-for-bit reproducible."
                    ))
                    .color(DIM)
                    .small(),
                );
            }
        });
    });
}

/// The render-complete panel: the mix's facts, the reproducibility line
/// with its seed, and the next moves (play / reveal / export MP3).
fn draw_complete_panel(ctx: &egui::Context, mix: &MixDone, acts: &mut Vec<Act>) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(ui.available_height() * 0.18);
        ui.vertical_centered(|ui| {
            ui.label(bungee("✓", 30.0, GREEN));
            ui.add_space(4.0);
            ui.label(bungee("mix rendered", 20.0, INK));
            ui.add_space(4.0);
            let name = mix
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("mix.wav");
            ui.label(egui::RichText::new(name).color(AMBER).strong());
            ui.add_space(12.0);

            let mins = (mix.secs / 60.0).floor() as u64;
            let secs = (mix.secs % 60.0).floor() as u64;
            ui.horizontal(|ui| {
                let total: f32 = 420.0;
                ui.add_space((ui.available_width() - total).max(0.0) / 2.0);
                for (k, v, amber) in [
                    ("length", format!("{mins}:{secs:02}"), false),
                    ("tempo", format!("{:.0} BPM", mix.master_bpm), true),
                    ("phrases", format!("{}", mix.sections), false),
                    ("format", "WAV · 44.1k".to_string(), false),
                ] {
                    ui.label(egui::RichText::new(k).color(DIM).small());
                    ui.label(
                        egui::RichText::new(v)
                            .color(if amber { AMBER } else { INK })
                            .strong(),
                    );
                    ui.add_space(14.0);
                }
            });
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format!(
                    "same sequence → same mix, bit for bit · seed {:#06x}",
                    mix.seed & 0xffff
                ))
                .color(DIM)
                .small(),
            );
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                let total: f32 = 430.0;
                ui.add_space((ui.available_width() - total).max(0.0) / 2.0);
                if ui
                    .button(egui::RichText::new("▶ play mix").color(GREEN).strong())
                    .clicked()
                {
                    acts.push(Act::Preview(mix.path.clone()));
                }
                if ui.button("reveal in library").clicked() {
                    acts.push(Act::Select(mix.path.clone()));
                    acts.push(Act::DismissMix);
                }
                if ui.button("export → MP3").clicked() {
                    acts.push(Act::ExportMp3(mix.path.clone()));
                }
                if ui
                    .button(egui::RichText::new("✕").color(DIM))
                    .on_hover_text("dismiss")
                    .clicked()
                {
                    acts.push(Act::DismissMix);
                }
            });
        });
    });
}

/// The sequence line (bottom panel): the product's only arranging
/// surface, in the big-poppa shape — status pills, the autosave note,
/// the master mini-readout, the render button (with its working state
/// and progress strip), and chips with order numbers, tempo badges,
/// pseudo-wave minis, and › swap separators.
fn draw_sequence_line(
    ctx: &egui::Context,
    sequence: &Sequence,
    beats: &BeatCache,
    working: Option<f32>,
    master: Option<&(f32, String)>,
    acts: &mut Vec<Act>,
) {
    let untapped = sequence
        .entries()
        .iter()
        .filter(|p| !beats.has_beats(p))
        .count();
    let ready = !sequence.is_empty() && untapped == 0;

    egui::TopBottomPanel::bottom("sequence")
        .exact_height(128.0)
        .show(ctx, |ui| {
            ui.add_space(5.0);
            // ── header ──
            ui.horizontal(|ui| {
                ui.label(bungee("sequence", 14.0, AMBER));
                if let Some(pct) = working {
                    ui.label(
                        egui::RichText::new(format!("● rendering… {pct:.0}%"))
                            .color(AMBER)
                            .small(),
                    );
                } else if sequence.is_empty() {
                    ui.label(
                        egui::RichText::new("empty — drag tracks in to start")
                            .color(DIM)
                            .small(),
                    );
                } else if ready {
                    ui.label(egui::RichText::new("✓ ready to render").color(GREEN).small());
                } else {
                    ui.label(
                        egui::RichText::new(format!(
                            "◷ {untapped} track{} need beats",
                            if untapped == 1 { "" } else { "s" }
                        ))
                        .color(AMBER)
                        .small(),
                    );
                }
                ui.label(
                    egui::RichText::new("· autosaved · sequence.txt")
                        .color(DIM)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if working.is_none() {
                        let btn = egui::Button::new(
                            egui::RichText::new("▶ render mix")
                                .color(if ready { GROUND } else { DIM })
                                .strong(),
                        )
                        .fill(if ready { AMBER } else { PANEL });
                        if ui
                            .add_enabled(ready, btn)
                            .on_hover_text("render the sequence to a mix (WAV in the library)")
                            .clicked()
                        {
                            acts.push(Act::RenderMix);
                        }
                    }
                    ui.add_space(8.0);
                    match master {
                        Some((bpm, _)) => ui.label(
                            egui::RichText::new(format!("master {bpm:.0} BPM"))
                                .color(AMBER)
                                .small(),
                        ),
                        None => ui.label(egui::RichText::new("master — BPM").color(DIM).small()),
                    };
                });
            });
            // progress strip under the header while rendering
            if let Some(pct) = working {
                let (bar, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 3.0),
                    egui::Sense::hover(),
                );
                let p = ui.painter_at(bar);
                p.rect_filled(bar, 0.0, PANEL);
                let mut fill = bar;
                fill.set_width(bar.width() * (pct / 100.0).clamp(0.0, 1.0));
                p.rect_filled(fill, 0.0, AMBER.gamma_multiply(0.85));
            }
            ui.add_space(4.0);

            let track_drag = egui::DragAndDrop::has_payload_of_type::<DragTrack>(ui.ctx());
            let entry_drag = egui::DragAndDrop::has_payload_of_type::<DragEntry>(ui.ctx());
            let n = sequence.len();

            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    if sequence.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "＋  drag tracks here to build your mix — the first track                                  sets the master tempo · repeats welcome",
                            )
                            .color(DIM),
                        );
                    }
                    for (i, path) in sequence.entries().iter().enumerate() {
                        if i > 0 {
                            ui.label(egui::RichText::new("›").color(DIM));
                        }
                        let tempo = beats
                            .get(path)
                            .and_then(|m| TermKrushApp::bpm_from_beats(&m.frames, m.sample_rate));
                        draw_sequence_chip(ui, i, path, tempo, acts);
                    }
                    // the tail: drop a library track to append, or an entry
                    // to send it to the end
                    if !sequence.is_empty() {
                        ui.label(egui::RichText::new("›").color(DIM));
                    }
                    let w = ui.available_width().max(96.0);
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(w.min(140.0), 72.0), egui::Sense::hover());
                    let p = ui.painter_at(rect);
                    p.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, LINE));
                    p.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "＋ drop",
                        egui::FontId::proportional(11.0),
                        DIM,
                    );
                    let hovered = matches!(
                        ui.input(|inp| inp.pointer.interact_pos()),
                        Some(pp) if rect.contains(pp)
                    );
                    if (track_drag || entry_drag) && hovered {
                        p.rect_filled(rect, 4.0, GREEN.gamma_multiply(0.18));
                        p.rect_stroke(rect, 4.0, egui::Stroke::new(1.5, GREEN));
                        if ui.input(|inp| inp.pointer.any_released()) {
                            if let Some(d) = egui::DragAndDrop::take_payload::<DragTrack>(ui.ctx())
                            {
                                acts.push(Act::SeqInsert {
                                    idx: n,
                                    path: d.0.clone(),
                                });
                            } else if let Some(d) =
                                egui::DragAndDrop::take_payload::<DragEntry>(ui.ctx())
                            {
                                acts.push(Act::SeqMove {
                                    from: d.0,
                                    to: n.saturating_sub(1),
                                });
                            }
                        }
                    }
                });
            });
        });
}

/// One sequence chip: order number + name + ✕ on top, tempo (or the
/// click-to-tap needs-beats badge) + "sets tempo" on the master below,
/// and a deterministic pseudo-waveform mini — seeded from the path, the
/// way the design seeds its waves (it stands in for the audio, cheaply).
fn draw_sequence_chip(
    ui: &mut egui::Ui,
    i: usize,
    path: &Path,
    tempo: Option<f32>,
    acts: &mut Vec<Act>,
) {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("track");
    let is_master = i == 0;
    let flagged = tempo.is_none();
    let frame = egui::Frame::group(ui.style())
        .fill(PANEL)
        .stroke(egui::Stroke::new(
            1.0,
            if is_master {
                AMBER
            } else if flagged {
                AMBER.gamma_multiply(0.5)
            } else {
                LINE
            },
        ))
        .inner_margin(egui::Margin::symmetric(6.0, 4.0));
    let inner = frame.show(ui, |ui| {
        ui.set_width(140.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                // the number + name strip is the drag handle for reordering
                let id = egui::Id::new(("seq-entry", i));
                let resp = ui
                    .dnd_drag_source(id, DragEntry(i), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}", i + 1))
                                .color(GREEN)
                                .strong(),
                        );
                        ui.add(
                            egui::Label::new(egui::RichText::new(stem).color(AMBER).strong())
                                .truncate(),
                        );
                    })
                    .response;
                resp.on_hover_text(format!("{stem} · drag to reorder"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_btn(ui, ph::X, "remove from the sequence") {
                        acts.push(Act::SeqRemove(i));
                    }
                });
            });
            ui.horizontal(|ui| {
                match tempo {
                    Some(b) => {
                        ui.label(
                            egui::RichText::new(format!("{b:.0} BPM"))
                                .color(GREEN)
                                .small(),
                        );
                    }
                    None => {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("needs beats ✎").color(AMBER).small(),
                                )
                                .frame(false),
                            )
                            .on_hover_text("tap this track's beats")
                            .clicked()
                        {
                            acts.push(Act::EditTrack(path.to_path_buf()));
                        }
                    }
                }
                if is_master {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("sets tempo").color(AMBER).small());
                    });
                }
            });
            // pseudo-wave mini, seeded by the path
            let (mini, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
            let p = ui.painter_at(mini);
            let mut seed: u64 = 0xcbf2_9ce4_8422_2325;
            for b in path.to_string_lossy().bytes() {
                seed ^= b as u64;
                seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
            }
            let bars = 26usize;
            let bw = mini.width() / bars as f32;
            let color = if flagged {
                DIM
            } else {
                GREEN.gamma_multiply(0.8)
            };
            for k in 0..bars {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let r = ((seed >> 33) as f32) / (u32::MAX as f32 / 2.0);
                let env = 0.4
                    + 0.6
                        * ((k as f32 / bars as f32) * std::f32::consts::PI * 3.0)
                            .sin()
                            .abs();
                let h = (0.16 + 0.7 * r.fract()) * env;
                let x = mini.left() + k as f32 * bw;
                let hh = mini.height() * h.clamp(0.08, 1.0);
                p.line_segment(
                    [
                        egui::pos2(x, mini.bottom() - hh),
                        egui::pos2(x, mini.bottom()),
                    ],
                    egui::Stroke::new((bw - 1.0).max(1.0), color),
                );
            }
        });
    });
    // the whole chip accepts drops: a library track inserts before this
    // entry; a dragged entry lands at this position
    let rect = inner.response.rect;
    let hovered =
        matches!(ui.input(|inp| inp.pointer.interact_pos()), Some(pp) if rect.contains(pp));
    let track_drag = egui::DragAndDrop::has_payload_of_type::<DragTrack>(ui.ctx());
    let entry_drag = egui::DragAndDrop::has_payload_of_type::<DragEntry>(ui.ctx());
    if (track_drag || entry_drag) && hovered {
        ui.painter().line_segment(
            [rect.left_top(), rect.left_bottom()],
            egui::Stroke::new(3.0, GREEN),
        );
        ui.painter()
            .rect_stroke(rect, 4.0, egui::Stroke::new(1.0, GREEN.gamma_multiply(0.6)));
        if ui.input(|inp| inp.pointer.any_released()) {
            if let Some(d) = egui::DragAndDrop::take_payload::<DragTrack>(ui.ctx()) {
                acts.push(Act::SeqInsert {
                    idx: i,
                    path: d.0.clone(),
                });
            } else if let Some(d) = egui::DragAndDrop::take_payload::<DragEntry>(ui.ctx()) {
                if d.0 != i {
                    acts.push(Act::SeqMove { from: d.0, to: i });
                }
            }
        }
    }
}

/// A dimmed full-window overlay with a spinner while tracks are decoding, so a
/// slow load doesn't feel frozen. Non-blocking — it's just paint.
fn draw_loading_overlay(ctx: &egui::Context, n: usize) {
    let label = if n > 1 {
        format!("loading {n} tracks…")
    } else {
        "loading…".to_string()
    };
    draw_busy_overlay(ctx, &label);
}

/// The dimmed spinner overlay itself — shared by loading and rendering.
fn draw_busy_overlay(ctx: &egui::Context, label: &str) {
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
                ui.label(bungee(label.to_string(), 14.0, INK));
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

/// A clean icon-only button (no frame). Returns true on click.
fn icon_btn(ui: &mut egui::Ui, icon: &str, tip: &str) -> bool {
    ui.add(egui::Button::new(egui::RichText::new(icon).size(17.0)).frame(false))
        .on_hover_cursor(egui::CursorIcon::Default)
        .on_hover_text(tip)
        .clicked()
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
    beats: &BeatCache,
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
                        let tempo = beats
                            .get(&e.path)
                            .and_then(|m| TermKrushApp::bpm_from_beats(&m.frames, m.sample_rate));
                        draw_track_row(ui, e, sel, renaming, bad, playing, tempo, acts);
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

/// A track row: drag source (move to folder), click to select, double-click
/// rename, pencil to open the beat-tap editor. Tapped tracks show their
/// fitted tempo.
#[allow(clippy::too_many_arguments)]
fn draw_track_row(
    ui: &mut egui::Ui,
    e: &termkrush_core::library::CrateEntry,
    sel: &Option<PathBuf>,
    renaming: &mut Option<(PathBuf, String)>,
    bad: bool,
    playing: bool,
    tempo: Option<f32>,
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
        // Open the beat-tap clip editor on this track.
        let pencil =
            egui::Button::new(egui::RichText::new(ph::PENCIL_SIMPLE).size(15.0).color(INK))
                .frame(false);
        if ui
            .add_enabled(!bad, pencil)
            .on_hover_cursor(egui::CursorIcon::Default)
            .on_hover_text("tap beats / trim")
            .clicked()
        {
            acts.push(Act::EditTrack(e.path.clone()));
        }
        // The name is the drag source (move); click selects, dbl-click renames.
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
        // A tapped track wears its tempo; marks are forever (cached).
        if let Some(b) = tempo {
            ui.label(egui::RichText::new(format!("{b:.0}")).color(GREEN).small())
                .on_hover_text(format!("tapped · {b:.1} BPM"));
        }
    });
}

/// The beat-tap stage (central): the big-poppa design's heart. A bar
/// ruler over the waveform, the fitted beat grid (downbeats numbered),
/// the user's raw taps as carets below, trim handles + region shading,
/// and a fit-stats footer: tempo · downbeat · taps · residual, with the
/// saved/fitting-live pill and the ↓ tap-key affordance.
#[allow(clippy::too_many_arguments)]
fn draw_clip_editor(
    ctx: &egui::Context,
    mixer: &Mixer,
    track: &str,
    i: usize,
    wave: &[(f32, f32)],
    beats: &[u64],
    sample_rate: u32,
    saved: bool,
    acts: &mut Vec<Act>,
) {
    // Provisional (amber) until the marks match what's cached; green once
    // saved. The grid the engine will use is always the FITTED one.
    let tone = if saved { GREEN } else { AMBER };
    let fit = fit_grid(beats);
    let playing = mixer.pad_is_sounding(i);

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(6.0);

        // ── header: tag · filename · hint · play-time ──
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(if playing && !saved {
                    "tap beats"
                } else {
                    "beats"
                })
                .color(GROUND)
                .background_color(tone)
                .small(),
            );
            ui.label(bungee(track, 15.0, AMBER));
            ui.label(
                egui::RichText::new("· trim is non-destructive · tapped once, kept forever")
                    .color(DIM)
                    .small(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let len = mixer.pad_clip_frames(i).max(1);
                let secs = |f: usize| f as f64 / mixer.sample_rate().max(1) as f64;
                let pos = mixer.pad_play_pos(i).unwrap_or(0);
                let m = |s: f64| format!("{}:{:02}", (s / 60.0) as u64, (s % 60.0) as u64);
                ui.label(
                    egui::RichText::new(format!("play {} / {}", m(secs(pos)), m(secs(len))))
                        .color(DIM)
                        .small(),
                );
            });
        });
        ui.add_space(4.0);

        let len = mixer.pad_clip_frames(i).max(1);
        let (inp, out) = mixer.pad_trim(i);

        // ── the stage: ruler + waveform + grid + taps + handles ──
        const RULER_H: f32 = 16.0;
        const TAPLANE_H: f32 = 18.0;
        let width = ui.available_width();
        let height = (ui.available_height() - 96.0).clamp(120.0, 420.0);
        let (stage, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
        let painter = ui.painter_at(stage);
        painter.rect_filled(stage, 4.0, GROUND);
        painter.rect_stroke(stage, 4.0, egui::Stroke::new(1.0, LINE));
        let wave_top = stage.top() + RULER_H;
        let wave_bot = stage.bottom() - TAPLANE_H;
        let mid = (wave_top + wave_bot) / 2.0;
        let amp = (wave_bot - wave_top) * 0.45;
        let x_of = |frame: usize| stage.left() + (frame as f32 / len as f32) * stage.width();

        // waveform
        let cols = stage.width() as usize;
        for c in 0..cols.min(if wave.is_empty() { 0 } else { cols }) {
            let (lo, hi) = wave[c * wave.len() / cols.max(1)];
            let x = stage.left() + c as f32;
            let frame = (c as f32 / cols.max(1) as f32 * len as f32) as usize;
            let inside = frame >= inp && frame < out;
            let color = if inside { AMBER } else { DIM };
            painter.line_segment(
                [egui::pos2(x, mid - hi * amp), egui::pos2(x, mid - lo * amp)],
                egui::Stroke::new(1.0, color),
            );
        }
        // region shading outside the trim
        for (a, b) in [(stage.left(), x_of(inp)), (x_of(out), stage.right())] {
            if b > a {
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(a, wave_top), egui::pos2(b, wave_bot)),
                    0.0,
                    egui::Color32::from_black_alpha(110),
                );
            }
        }

        // fitted beat grid: thin lines on every beat, stronger + numbered
        // on downbeats (the grid the engine will lock to).
        if let Some((phase, interval)) = fit {
            let mut k = 0u64;
            loop {
                let fpos = phase + k as f64 * interval;
                if fpos >= len as f64 {
                    break;
                }
                if fpos >= 0.0 {
                    let x = x_of(fpos as usize);
                    let down = k % 4 == 0;
                    painter.line_segment(
                        [egui::pos2(x, wave_top), egui::pos2(x, wave_bot)],
                        egui::Stroke::new(
                            if down { 1.6 } else { 1.0 },
                            tone.gamma_multiply(if down { 0.9 } else { 0.45 }),
                        ),
                    );
                    if down {
                        painter.text(
                            egui::pos2(x + 2.0, stage.top() + 2.0),
                            egui::Align2::LEFT_TOP,
                            format!("{}", k / 4 + 1),
                            egui::FontId::proportional(9.0),
                            DIM,
                        );
                    }
                }
                k += 1;
                if k > 4096 {
                    break;
                }
            }
        }
        // the user's raw taps: carets in the lane below the waveform —
        // the fit averages them, the carets show what it averaged.
        painter.text(
            egui::pos2(stage.left() + 4.0, wave_bot + 2.0),
            egui::Align2::LEFT_TOP,
            "↓ taps",
            egui::FontId::proportional(9.0),
            DIM,
        );
        for &b in beats {
            if (b as usize) < len {
                let x = x_of(b as usize);
                painter.text(
                    egui::pos2(x, wave_bot + 1.0),
                    egui::Align2::CENTER_TOP,
                    "▾",
                    egui::FontId::proportional(11.0),
                    tone,
                );
            }
        }
        // click the waveform to add/remove a tap at that frame
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if p.y >= wave_top && p.y <= wave_bot {
                    let fr = (((p.x - stage.left()) / stage.width()).clamp(0.0, 1.0) * len as f32)
                        as u64;
                    acts.push(Act::ToggleBeat(i, fr));
                }
            }
        }

        // trim handles
        for (is_out, frame) in [(false, inp), (true, out)] {
            let hx = x_of(frame);
            let handle = egui::Rect::from_min_max(
                egui::pos2(hx - 5.0, wave_top),
                egui::pos2(hx + 5.0, wave_bot),
            );
            let id = ui.id().with(("ce_handle", is_out));
            let hresp = ui
                .interact(handle, id, egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            painter.line_segment(
                [egui::pos2(hx, wave_top), egui::pos2(hx, wave_bot)],
                egui::Stroke::new(2.0, AMBER),
            );
            let knob = egui::Rect::from_center_size(
                egui::pos2(hx, wave_top + 7.0),
                egui::vec2(10.0, 12.0),
            );
            painter.rect_filled(knob, 3.0, AMBER);
            painter.rect_filled(
                egui::Rect::from_center_size(knob.center(), egui::vec2(2.0, 6.0)),
                0.0,
                GROUND,
            );
            if hresp.dragged() {
                if let Some(p) = hresp.interact_pointer_pos() {
                    let fr = (((p.x - stage.left()) / stage.width()).clamp(0.0, 1.0) * len as f32)
                        as usize;
                    if is_out {
                        acts.push(Act::SetTrimOut(i, fr));
                    } else {
                        acts.push(Act::SetTrimIn(i, fr));
                    }
                }
            }
        }

        // playhead while auditioning — what you tap against
        if playing {
            if let Some(pos) = mixer.pad_play_pos(i) {
                let px = x_of(pos.min(len));
                painter.line_segment(
                    [egui::pos2(px, stage.top()), egui::pos2(px, stage.bottom())],
                    egui::Stroke::new(1.5, GREEN),
                );
            }
        }
        // ↓ taps a beat mark at the playhead while auditioning
        if playing && ctx.input(|inp_| inp_.key_pressed(egui::Key::ArrowDown)) {
            acts.push(Act::TapBeat(i));
        }

        ui.add_space(8.0);

        // ── footer: fit stats · pill · tap key · play/save ──
        ui.horizontal(|ui| {
            let bpm = fit.map(|(_p, iv)| sample_rate as f64 * 60.0 / iv);
            let downbeat = fit.map(|(p, _iv)| p / sample_rate as f64);
            // residual: RMS distance of raw taps from the fitted grid, ms
            let residual = fit.map(|(p, iv)| {
                let mut sum = 0.0f64;
                for &b in beats {
                    let k = ((b as f64 - p) / iv).round();
                    let d = b as f64 - (p + k * iv);
                    sum += d * d;
                }
                (sum / beats.len().max(1) as f64).sqrt() / sample_rate as f64 * 1000.0
            });
            let kv = |ui: &mut egui::Ui, k: &str, v: String, c: egui::Color32| {
                ui.label(egui::RichText::new(k).color(DIM).small());
                ui.label(egui::RichText::new(v).color(c).strong());
                ui.add_space(10.0);
            };
            kv(
                ui,
                "tempo",
                bpm.map(|b| format!("{b:.1} bpm")).unwrap_or("—".into()),
                tone,
            );
            kv(
                ui,
                "downbeat",
                downbeat.map(|d| format!("{d:.3} s")).unwrap_or("—".into()),
                INK,
            );
            kv(ui, "taps fit", format!("{}", beats.len()), INK);
            kv(
                ui,
                "residual",
                residual
                    .map(|r| format!("±{r:.0} ms"))
                    .unwrap_or("—".into()),
                INK,
            );
            if saved {
                ui.label(
                    egui::RichText::new("✓ saved to beats.txt")
                        .color(GREEN)
                        .small(),
                );
                ui.label(
                    egui::RichText::new("marks follow renames & moves")
                        .color(DIM)
                        .small(),
                );
            } else if fit.is_some() {
                ui.label(egui::RichText::new("● fitting live").color(AMBER).small());
                ui.label(
                    egui::RichText::new("keep tapping — least-squares averages every ↓")
                        .color(DIM)
                        .small(),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(
                        egui::RichText::new(if saved { "saved ✓" } else { "save" })
                            .color(if saved { GREEN } else { GROUND })
                            .background_color(if saved {
                                egui::Color32::TRANSPARENT
                            } else {
                                AMBER
                            })
                            .strong(),
                    )
                    .on_hover_text("keep the trim + beats and close the editor")
                    .clicked()
                {
                    acts.push(Act::CloseClip);
                }
                if ui
                    .button("save to library")
                    .on_hover_text("write the trimmed clip to the library as a WAV")
                    .clicked()
                {
                    acts.push(Act::ExportClip(i));
                }
                if ui
                    .button(if playing { "⏸ pause" } else { "▶ play" })
                    .clicked()
                {
                    acts.push(Act::AuditionSel(i));
                }
                if ui
                    .button(egui::RichText::new("clear").color(DIM))
                    .on_hover_text("clear the marks and re-tap")
                    .clicked()
                {
                    acts.push(Act::ClearBeats(i));
                }
                // the tap-key affordance: the one control that matters
                ui.label(
                    egui::RichText::new(" ↓ tap on beat ")
                        .color(if playing { GROUND } else { DIM })
                        .background_color(if playing { tone } else { PANEL }),
                );
            });
        });
    });
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
