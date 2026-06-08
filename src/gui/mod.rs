//! The egui/eframe desktop front-end — the mouse-first replacement for the
//! TUI (see the 2026-06-08 GUI pivot in `.am/inception.md`). This foundation
//! story stands up the window, the CRT amber/green theme, the audio pump, and
//! the three zones (timeline strip · library · pad grid) rendered from real
//! `termkrush-core` state. Interactions land in the panel stories that follow.

use std::path::PathBuf;

use eframe::egui;
use termkrush_core::audio::AudioOutput;
use termkrush_core::config::Config;
use termkrush_core::library::Crate;
use termkrush_core::mix::{Mixer, PadKind, PADS};

// CRT identity, modernized: near-black ground, green text, amber accents.
const GREEN: egui::Color32 = egui::Color32::from_rgb(0x3a, 0xf0, 0x6a);
const AMBER: egui::Color32 = egui::Color32::from_rgb(0xff, 0xb0, 0x14);
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

/// The whole app: the engine, the audio sink, and the browsed library.
pub struct TermKrushApp {
    mixer: Mixer,
    crate_lib: Crate,
    /// Source path loaded on each pad (for the cell's track name).
    pad_source: [Option<PathBuf>; PADS],
    producer: Option<rtrb::Producer<f32>>,
    _audio: Option<AudioOutput>,
    out_channels: usize,
    scratch: Vec<f32>,
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

        Self {
            mixer,
            crate_lib,
            pad_source: Default::default(),
            producer,
            _audio: audio,
            out_channels: channels.max(1),
            scratch: Vec::new(),
        }
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

    fn pad_track_name(&self, i: usize) -> &str {
        self.pad_source[i]
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("")
    }
}

impl eframe::App for TermKrushApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_audio();
        ctx.request_repaint(); // keep the audio ring fed in real time

        draw_timeline_strip(ctx, &self.mixer);
        draw_library(ctx, &self.crate_lib);
        draw_pad_grid(self, ctx);
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

fn draw_library(ctx: &egui::Context, lib: &Crate) {
    egui::SidePanel::left("library")
        .resizable(true)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("LIBRARY").color(AMBER).strong());
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for e in lib.entries() {
                    let label = if e.is_dir && e.name != ".." {
                        format!("📁 {}", e.name)
                    } else {
                        e.name.clone()
                    };
                    ui.label(label);
                }
                if lib.is_empty() {
                    ui.label(egui::RichText::new("(empty — set crate_root)").weak());
                }
            });
        });
}

fn draw_pad_grid(app: &TermKrushApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(6.0);
        ui.label(egui::RichText::new("PADS").color(AMBER).strong());
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
                    draw_pad_cell(app, ui, i, cell_w);
                    if (i + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
    });
}

fn draw_pad_cell(app: &TermKrushApp, ui: &mut egui::Ui, i: usize, w: f32) {
    let loaded = app.mixer.pad_loaded(i);
    let kind = match app.mixer.pad_kind(i) {
        PadKind::OneShot => "1shot",
        PadKind::Loop => "loop",
        PadKind::Scratch => "scratch",
    };
    egui::Frame::group(ui.style())
        .fill(PANEL)
        .stroke(egui::Stroke::new(
            1.0,
            if loaded {
                AMBER
            } else {
                GREEN.gamma_multiply(0.4)
            },
        ))
        .show(ui, |ui| {
            ui.set_width(w - 16.0);
            ui.set_min_height(80.0);
            ui.vertical(|ui| {
                let title = app.pad_track_name(i);
                let head = if title.is_empty() {
                    format!("{}", i + 1)
                } else {
                    format!("{}  {title}", i + 1)
                };
                ui.label(egui::RichText::new(head).color(AMBER).strong());
                if loaded {
                    ui.label(format!("{kind} · {:.0}%", app.mixer.pad_gain(i) * 100.0));
                } else {
                    ui.label(egui::RichText::new("empty").weak());
                }
            });
        });
}
