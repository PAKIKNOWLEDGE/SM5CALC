#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ffi;
mod sm;

use eframe::egui;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use sm::{Chart, Song};

const INK: egui::Color32 = egui::Color32::from_rgb(0x16, 0x14, 0x1C);
const PAPER: egui::Color32 = egui::Color32::from_rgb(0xE7, 0xE1, 0xD6);
const GOLD: egui::Color32 = egui::Color32::from_rgb(0xE3, 0xB3, 0x41);
const MUTED: egui::Color32 = egui::Color32::from_rgb(0x8A, 0x84, 0x78);
const BAR: egui::Color32 = egui::Color32::from_rgb(0x8E, 0x9A, 0xAB);
const TRACK: egui::Color32 = egui::Color32::from_rgb(0x2A, 0x25, 0x33);
const WARN: egui::Color32 = egui::Color32::from_rgb(0xD4, 0x53, 0x4E);

enum Status {
    Ready,
    Loading(String),
    Failed(String),
}

enum Msg {
    Song { id: u64, song: Song },
    Failed { id: u64, reason: String },
}

struct App {
    song: Option<Song>,
    selected: usize,
    status: Status,
    version: i32,
    job_id: u64,
    jobs: Sender<(u64, PathBuf)>,
    incoming: Receiver<Msg>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        load_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);
        let (jobs, job_rx) = mpsc::channel();
        let (msg_tx, incoming) = mpsc::channel();
        std::thread::spawn(move || worker(job_rx, msg_tx));
        Self {
            song: None,
            selected: 0,
            status: Status::Ready,
            version: ffi::calc_version(),
            job_id: 0,
            jobs,
            incoming,
        }
    }

    fn submit(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.status = Status::Failed("这是文件夹。把单个 .sm 拖进来。".to_owned());
            return;
        }
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "ssc" {
            self.status = Status::Failed("目前只读 .sm。这个文件是 .ssc。".to_owned());
            return;
        }
        if ext != "sm" {
            self.status = Status::Failed("需要 .sm 文件。".to_owned());
            return;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "谱面".to_owned());
        self.job_id += 1;
        self.status = Status::Loading(name);
        let _ = self.jobs.send((self.job_id, path));
    }

    fn poll(&mut self) {
        while let Ok(msg) = self.incoming.try_recv() {
            let (id, apply) = match msg {
                Msg::Song { id, song } => (id, Some(song)),
                Msg::Failed { id, reason } => {
                    if id == self.job_id {
                        self.status = Status::Failed(reason);
                    }
                    (id, None)
                }
            };
            if id != self.job_id {
                continue;
            }
            if let Some(song) = apply {
                self.selected = 0;
                self.song = Some(song);
                self.status = Status::Ready;
            }
        }
    }
}

fn worker(jobs: Receiver<(u64, PathBuf)>, outgoing: Sender<Msg>) {
    while let Ok((id, path)) = jobs.recv() {
        let msg = match std::fs::read(&path) {
            Ok(bytes) => match sm::parse_sm(&path, &bytes) {
                Ok(mut song) => {
                    let mut failure = None;
                    for chart in &mut song.charts {
                        match ffi::rate_notes(&chart.notes, chart.key_count) {
                            Ok(ratings) => chart.ratings = ratings,
                            Err(reason) => {
                                failure = Some(reason);
                                break;
                            }
                        }
                    }
                    match failure {
                        Some(reason) => Msg::Failed { id, reason },
                        None => Msg::Song { id, song },
                    }
                }
                Err(reason) => Msg::Failed { id, reason },
            },
            Err(error) => Msg::Failed {
                id,
                reason: format!("读不了这个文件：{error}"),
            },
        };
        if outgoing.send(msg).is_err() {
            break;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if matches!(self.status, Status::Loading(_)) {
            ctx.request_repaint_after(Duration::from_millis(80));
        }

        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            self.submit(path);
        }

        let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
        egui::CentralPanel::default().show(ctx, |ui| {
            top_bar(ui, self);
            ui.add_space(8.0);
            match &self.status {
                Status::Loading(name) => {
                    ui.label(egui::RichText::new(format!("正在计算 {name}")).color(GOLD));
                    ui.add_space(8.0);
                }
                Status::Failed(reason) => {
                    ui.label(egui::RichText::new(reason).color(WARN));
                    ui.add_space(8.0);
                }
                Status::Ready => {}
            }

            let selected = self.selected;
            let updated = self
                .song
                .as_ref()
                .map(|song| song_view(ui, song, selected));
            if let Some(updated) = updated {
                self.selected = updated;
            } else if !matches!(self.status, Status::Failed(_)) {
                empty_view(ui, hovering);
            }

            if hovering {
                let rect = ui.max_rect().shrink(4.0);
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(2.0, GOLD),
                    egui::StrokeKind::Inside,
                );
            }
        });
    }
}

fn top_bar(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new("打开 .sm").min_size(egui::vec2(108.0, 32.0)))
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("StepMania", &["sm"])
                .pick_file()
            {
                app.submit(path);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("计算器 {}", app.version))
                    .color(MUTED)
                    .size(14.0),
            );
        });
    });
}

fn empty_view(ui: &mut egui::Ui, hovering: bool) {
    ui.add_space(72.0);
    ui.vertical_centered(|ui| {
        let headline = if hovering {
            "松开，开始计算"
        } else {
            "把 .sm 拖到窗口里"
        };
        ui.label(egui::RichText::new(headline).size(32.0).color(PAPER));
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("会给出 0.70 到 2.00 倍的综合，以及七个单项。谱面上手写的定数只作对照。")
                .color(MUTED)
                .size(16.0),
        );
    });
}

fn song_view(ui: &mut egui::Ui, song: &Song, mut selected: usize) -> usize {
    if song.charts.is_empty() {
        return selected;
    }
    selected = selected.min(song.charts.len() - 1);

    egui::ScrollArea::vertical()
        .id_salt("song")
        .show(ui, |ui| {
            ui.label(egui::RichText::new(&song.title).size(28.0).color(PAPER));
            ui.label(egui::RichText::new(&song.artist).size(16.0).color(MUTED));
            for warning in &song.warnings {
                ui.label(egui::RichText::new(warning).color(WARN));
            }
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                for (index, candidate) in song.charts.iter().enumerate() {
                    let meter = if candidate.meter.is_empty() {
                        candidate.difficulty.clone()
                    } else {
                        format!("{}  {}", candidate.difficulty, candidate.meter)
                    };
                    if ui.selectable_label(index == selected, meter).clicked() {
                        selected = index;
                    }
                }
            });

            ui.add_space(16.0);
            let chart = &song.charts[selected];
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(220.0);
                    ui.label(egui::RichText::new("1.00 倍").color(MUTED).size(14.0));
                    ui.label(
                        egui::RichText::new(format!("{:.2}", chart.ratings[3][0]))
                            .family(egui::FontFamily::Monospace)
                            .size(68.0)
                            .color(GOLD),
                    );
                    let meter_line = if chart.meter.is_empty() {
                        "谱面没有写定数。".to_owned()
                    } else {
                        format!("谱面写的定数是 {}。", chart.meter)
                    };
                    ui.label(egui::RichText::new(meter_line).color(MUTED));
                });
                ui.add_space(18.0);
                ui.vertical(|ui| {
                    skill_bars(ui, chart);
                });
            });

            ui.add_space(12.0);
            let kind = if chart.description.is_empty() {
                format!("{} 键    {}", chart.key_count, chart.steps_type)
            } else {
                format!(
                    "{} 键    {}    {}",
                    chart.key_count, chart.steps_type, chart.description
                )
            };
            ui.label(egui::RichText::new(kind).color(MUTED));
            ui.add_space(6.0);
            ui.label(egui::RichText::new(sm::blurb(chart)).size(16.0).color(PAPER));
            ui.add_space(18.0);
            ui.label(egui::RichText::new("各倍速").size(18.0));
            ui.add_space(6.0);
            rate_table(ui, chart);
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(song.path.display().to_string())
                    .color(MUTED)
                    .size(12.0),
            );
        });
    selected
}

fn skill_bars(ui: &mut egui::Ui, chart: &Chart) {
    let labels = sm::skill_labels(chart.key_count);
    let values = chart.ratings[3];
    let mut top = 1usize;
    let mut scale = 1.0f32;
    for index in 1..8 {
        if values[index] > values[top] {
            top = index;
        }
        scale = scale.max(values[index]);
    }
    if scale < 1.0 {
        scale = 1.0;
    }
    for index in 1..8 {
        let accent = index == top && values[index] > 0.0;
        let color = if accent { GOLD } else { PAPER };
        ui.horizontal(|ui| {
            ui.add_sized(
                [118.0, 20.0],
                egui::Label::new(egui::RichText::new(labels[index]).color(color)),
            );
            ui.add_sized(
                [58.0, 20.0],
                egui::Label::new(
                    egui::RichText::new(format!("{:.2}", values[index]))
                        .family(egui::FontFamily::Monospace)
                        .color(color),
                ),
            );
            let width = ui.available_width().max(24.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 10.0), egui::Sense::hover());
            let filled = (values[index] / scale).clamp(0.0, 1.0) * rect.width();
            let bar = egui::Rect::from_min_size(rect.min, egui::vec2(filled, rect.height()));
            ui.painter().rect_filled(rect, 0.0, TRACK);
            ui.painter()
                .rect_filled(bar, 0.0, if accent { GOLD } else { BAR });
        });
    }
}

fn rate_table(ui: &mut egui::Ui, chart: &Chart) {
    let labels = sm::skill_labels(chart.key_count);
    egui::Grid::new("rates")
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("倍速").color(MUTED));
            for label in labels {
                ui.label(egui::RichText::new(label).color(MUTED));
            }
            ui.end_row();
            for rate in 0..14 {
                let accent = rate == 3;
                let paint = |ui: &mut egui::Ui, text: String| {
                    let mut rich = egui::RichText::new(text).family(egui::FontFamily::Monospace);
                    if accent {
                        rich = rich.color(GOLD);
                    }
                    ui.label(rich);
                };
                paint(ui, sm::rate_label(rate));
                for skill in 0..8 {
                    paint(ui, format!("{:.2}", chart.ratings[rate][skill]));
                }
                ui.end_row();
            }
        });
}

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\Deng.ttf") {
        fonts
            .font_data
            .insert("deng".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "deng".to_owned());
        fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .unwrap()
            .push("deng".to_owned());
    }
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\consola.ttf") {
        fonts.font_data.insert(
            "consola".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .unwrap()
            .insert(0, "consola".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = INK;
    visuals.panel_fill = INK;
    visuals.faint_bg_color = egui::Color32::from_rgb(0x22, 0x1E, 0x2A);
    visuals.extreme_bg_color = egui::Color32::from_rgb(0x10, 0x0E, 0x14);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, PAPER);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, TRACK);
    visuals.widgets.inactive.bg_fill = TRACK;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, PAPER);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x3A, 0x33, 0x44);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, GOLD);
    visuals.widgets.active.bg_fill = GOLD;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, INK);
    visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(0xE3, 0xB3, 0x41, 0x55);
    visuals.selection.stroke = egui::Stroke::new(1.0, GOLD);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
    });
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 760.0])
            .with_min_inner_size([760.0, 520.0])
            .with_drag_and_drop(true)
            .with_title("SM5CALC"),
        ..Default::default()
    };
    eframe::run_native(
        "SM5CALC",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
