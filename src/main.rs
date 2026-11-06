#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ffi;
mod sm;

use eframe::egui;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use sm::{Chart, Song};

// Surface & Background Colors
const BG_BASE: egui::Color32 = egui::Color32::from_rgb(0x10, 0x12, 0x16);
const BG_CARD: egui::Color32 = egui::Color32::from_rgb(0x18, 0x1B, 0x24);
const BG_CARD_ALT: egui::Color32 = egui::Color32::from_rgb(0x21, 0x25, 0x32);
const BG_TRACK: egui::Color32 = egui::Color32::from_rgb(0x28, 0x2D, 0x3D);

// Border & Stroke Colors
const BORDER_SUBTLE: egui::Color32 = egui::Color32::from_rgb(0x2B, 0x31, 0x42);
const BORDER_ACTIVE: egui::Color32 = egui::Color32::from_rgb(0x3B, 0x82, 0xF6);

// Text Colors
const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xF3, 0xF4, 0xF6);
const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0x94, 0xA3, 0xB8);
const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(0x64, 0x74, 0x8B);

// Brand & Accent Colors
const ACCENT_AMBER: egui::Color32 = egui::Color32::from_rgb(0xF5, 0x9E, 0x0B);
const ACCENT_BLUE: egui::Color32 = egui::Color32::from_rgb(0x3B, 0x82, 0xF6);
const ACCENT_CYAN: egui::Color32 = egui::Color32::from_rgb(0x38, 0xBD, 0xF8);
const STATUS_DANGER: egui::Color32 = egui::Color32::from_rgb(0xEF, 0x44, 0x44);
const STATUS_SUCCESS: egui::Color32 = egui::Color32::from_rgb(0x10, 0xB9, 0x81);

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
            self.status = Status::Failed("所选路径为文件夹，请拖入单个 .sm 谱面文件。".to_owned());
            return;
        }
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "ssc" {
            self.status = Status::Failed("当前暂不支持 .ssc 格式，请选择 .sm 谱面文件。".to_owned());
            return;
        }
        if ext != "sm" {
            self.status = Status::Failed("格式不支持，仅可解析 .sm 谱面文件。".to_owned());
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
                reason: format!("读取文件失败: {error}"),
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
            ui.separator();
            ui.add_space(8.0);

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
                    egui::CornerRadius::same(8),
                    egui::Stroke::new(2.0, ACCENT_BLUE),
                    egui::StrokeKind::Inside,
                );
            }
        });
    }
}

fn top_bar(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("SM5CALC")
                .size(20.0)
                .strong()
                .color(TEXT_PRIMARY),
        );
        ui.add_space(6.0);

        let version_tag = format!("MinaCalc v{}", app.version);
        let tag_rect = ui
            .label(
                egui::RichText::new(version_tag)
                    .size(11.0)
                    .color(ACCENT_CYAN),
            )
            .rect;
        ui.painter().rect_stroke(
            tag_rect.expand2(egui::vec2(6.0, 2.0)),
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(0x38, 0xBD, 0xF8, 0x50)),
            egui::StrokeKind::Inside,
        );

        ui.add_space(16.0);

        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("📂 打开 .sm 谱面")
                        .size(13.0)
                        .color(TEXT_PRIMARY),
                )
                .min_size(egui::vec2(124.0, 32.0)),
            )
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("StepMania Simfile (*.sm)", &["sm"])
                .pick_file()
            {
                app.submit(path);
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match &app.status {
                Status::Loading(name) => {
                    ui.label(
                        egui::RichText::new(format!("⏳ 正在计算: {name}"))
                            .color(ACCENT_AMBER)
                            .size(13.0),
                    );
                }
                Status::Failed(reason) => {
                    ui.label(
                        egui::RichText::new(format!("⚠ {reason}"))
                            .color(STATUS_DANGER)
                            .size(13.0),
                    );
                }
                Status::Ready => {
                    if app.song.is_some() {
                        ui.label(
                            egui::RichText::new("✔ 解析完成")
                                .color(STATUS_SUCCESS)
                                .size(12.0),
                        );
                    }
                }
            }
        });
    });
}

fn empty_view(ui: &mut egui::Ui, hovering: bool) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        let card_frame = egui::Frame::default()
            .fill(if hovering { BG_CARD_ALT } else { BG_CARD })
            .stroke(egui::Stroke::new(
                if hovering { 2.0 } else { 1.0 },
                if hovering { ACCENT_BLUE } else { BORDER_SUBTLE },
            ))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(40, 48));

        card_frame.show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(if hovering { "📥" } else { "🎼" })
                        .size(48.0),
                );
                ui.add_space(16.0);
                let title = if hovering {
                    "松开鼠标，开始计算"
                } else {
                    "拖入 StepMania .sm 谱面文件"
                };
                ui.label(
                    egui::RichText::new(title)
                        .size(22.0)
                        .strong()
                        .color(if hovering { ACCENT_BLUE } else { TEXT_PRIMARY }),
                );
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(
                        "基于 Etterna 官方 MinaCalc 核心算法，秒级还原 Timing 状态机并计算 8 维 MSD 定数矩阵",
                    )
                    .color(TEXT_SECONDARY)
                    .size(13.0),
                );
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new("支持 4K、5K、6K、7K 模式 | 覆盖 0.70x ~ 2.00x 全速率")
                        .color(TEXT_MUTED)
                        .size(12.0),
                );
            });
        });
    });
}

fn badge(ui: &mut egui::Ui, text: impl Into<String>, bg: egui::Color32, fg: egui::Color32) {
    let frame = egui::Frame::default()
        .fill(bg)
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(8, 4));
    frame.show(ui, |ui| {
        ui.label(egui::RichText::new(text.into()).size(12.0).color(fg));
    });
}

fn song_view(ui: &mut egui::Ui, song: &Song, mut selected: usize) -> usize {
    if song.charts.is_empty() {
        return selected;
    }
    selected = selected.min(song.charts.len() - 1);

    egui::ScrollArea::vertical()
        .id_salt("song_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Song Header Card
            let header_frame = egui::Frame::default()
                .fill(BG_CARD)
                .stroke(egui::Stroke::new(1.0, BORDER_SUBTLE))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(16.0);

            header_frame.show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(&song.title)
                            .size(24.0)
                            .strong()
                            .color(TEXT_PRIMARY),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(&song.artist)
                            .size(14.0)
                            .color(TEXT_SECONDARY),
                    );

                    for warning in &song.warnings {
                        ui.add_space(6.0);
                        let warn_frame = egui::Frame::default()
                            .fill(egui::Color32::from_rgba_unmultiplied(0xEF, 0x44, 0x44, 0x22))
                            .stroke(egui::Stroke::new(1.0, STATUS_DANGER))
                            .corner_radius(egui::CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(10, 6));
                        warn_frame.show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("⚠ {warning}"))
                                    .color(STATUS_DANGER)
                                    .size(12.0),
                            );
                        });
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Difficulty Tabs / Pills
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new("谱面难度:")
                                .color(TEXT_MUTED)
                                .size(13.0),
                        );
                        ui.add_space(4.0);
                        for (index, candidate) in song.charts.iter().enumerate() {
                            let is_current = index == selected;
                            let title = if candidate.meter.is_empty() {
                                candidate.difficulty.clone()
                            } else {
                                format!("{} [{}]", candidate.difficulty, candidate.meter)
                            };

                            let (btn_bg, text_color) = if is_current {
                                (ACCENT_BLUE, egui::Color32::WHITE)
                            } else {
                                (BG_CARD_ALT, TEXT_SECONDARY)
                            };

                            let btn = egui::Button::new(
                                egui::RichText::new(title)
                                    .size(12.0)
                                    .strong()
                                    .color(text_color),
                            )
                            .fill(btn_bg)
                            .stroke(if is_current {
                                egui::Stroke::NONE
                            } else {
                                egui::Stroke::new(1.0, BORDER_SUBTLE)
                            })
                            .corner_radius(egui::CornerRadius::same(6));

                            if ui.add(btn).clicked() {
                                selected = index;
                            }
                        }
                    });
                });
            });

            ui.add_space(10.0);
            let chart = &song.charts[selected];

            // Metadata Badges Row
            ui.horizontal_wrapped(|ui| {
                badge(
                    ui,
                    format!("{} 键", chart.key_count),
                    BG_CARD_ALT,
                    ACCENT_CYAN,
                );
                badge(ui, &chart.steps_type, BG_CARD_ALT, TEXT_PRIMARY);
                if !chart.description.is_empty() {
                    badge(ui, &chart.description, BG_CARD_ALT, TEXT_MUTED);
                }
                badge(
                    ui,
                    format!("时长 {:.1}s", chart.duration),
                    BG_CARD_ALT,
                    TEXT_SECONDARY,
                );
                badge(
                    ui,
                    format!("音符数 {}", chart.note_count),
                    BG_CARD_ALT,
                    TEXT_SECONDARY,
                );
                badge(
                    ui,
                    format!("平均 {:.1} NPS", chart.avg_nps),
                    BG_CARD_ALT,
                    TEXT_SECONDARY,
                );
                badge(
                    ui,
                    format!("半秒峰值 {:.1} NPS", chart.peak_nps),
                    BG_CARD_ALT,
                    TEXT_SECONDARY,
                );
            });

            ui.add_space(10.0);

            // Core Rating & Skillset Card
            let main_card_frame = egui::Frame::default()
                .fill(BG_CARD)
                .stroke(egui::Stroke::new(1.0, BORDER_SUBTLE))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(18.0);

            main_card_frame.show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // Left Column: 1.00x Overall Score
                    ui.vertical(|ui| {
                        ui.set_min_width(210.0);
                        ui.label(
                            egui::RichText::new("1.00x 基准综合评分 (Overall MSD)")
                                .color(TEXT_MUTED)
                                .size(12.0),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!("{:.2}", chart.ratings[3][0]))
                                .family(egui::FontFamily::Monospace)
                                .size(58.0)
                                .strong()
                                .color(ACCENT_AMBER),
                        );
                        let meter_line = if chart.meter.is_empty() {
                            "制谱者未标注定数".to_owned()
                        } else {
                            format!("制谱标注定数: {} (仅供参考)", chart.meter)
                        };
                        ui.label(
                            egui::RichText::new(meter_line)
                                .color(TEXT_MUTED)
                                .size(12.0),
                        );
                    });

                    ui.add_space(20.0);

                    // Right Column: Skillset Breakdown Bars
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("7 维能力属性分布 (Skillsets)")
                                .color(TEXT_MUTED)
                                .size(12.0),
                        );
                        ui.add_space(6.0);
                        skill_bars(ui, chart);
                    });
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                // Summary Callout
                let blurb_frame = egui::Frame::default()
                    .fill(BG_CARD_ALT)
                    .stroke(egui::Stroke::new(1.0, BORDER_SUBTLE))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(14, 10));
                blurb_frame.show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(sm::blurb(chart))
                            .size(13.0)
                            .color(TEXT_PRIMARY),
                    );
                });
            });

            ui.add_space(16.0);

            // Rate Matrix Section
            ui.label(
                egui::RichText::new("各倍速定数矩阵 (0.70x ~ 2.00x)")
                    .size(16.0)
                    .strong()
                    .color(TEXT_PRIMARY),
            );
            ui.add_space(6.0);
            rate_table(ui, chart);

            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(format!("当前文件: {}", song.path.display()))
                    .color(TEXT_MUTED)
                    .size(11.0),
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
        let is_top = index == top && values[index] > 0.0;
        let color = if is_top { ACCENT_AMBER } else { TEXT_PRIMARY };
        let fill_color = if is_top { ACCENT_AMBER } else { ACCENT_CYAN };

        ui.horizontal(|ui| {
            ui.add_sized(
                [110.0, 20.0],
                egui::Label::new(
                    egui::RichText::new(labels[index])
                        .color(color)
                        .strong()
                        .size(13.0),
                ),
            );
            ui.add_sized(
                [52.0, 20.0],
                egui::Label::new(
                    egui::RichText::new(format!("{:.2}", values[index]))
                        .family(egui::FontFamily::Monospace)
                        .color(color)
                        .size(13.0),
                ),
            );
            let width = ui.available_width().max(40.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 10.0), egui::Sense::hover());
            let filled = (values[index] / scale).clamp(0.0, 1.0) * rect.width();
            let bar = egui::Rect::from_min_size(rect.min, egui::vec2(filled, rect.height()));
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(4), BG_TRACK);
            if filled > 0.0 {
                ui.painter()
                    .rect_filled(bar, egui::CornerRadius::same(4), fill_color);
            }
        });
    }
}

fn rate_table(ui: &mut egui::Ui, chart: &Chart) {
    let labels = sm::skill_labels(chart.key_count);
    let table_frame = egui::Frame::default()
        .fill(BG_CARD)
        .stroke(egui::Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12));

    table_frame.show(ui, |ui| {
        egui::Grid::new("rates")
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("音乐速率").strong().color(TEXT_MUTED));
                for label in labels {
                    ui.label(egui::RichText::new(label).strong().color(TEXT_MUTED));
                }
                ui.end_row();

                for rate in 0..14 {
                    let is_1x = rate == 3;
                    let text_color = if is_1x { ACCENT_AMBER } else { TEXT_SECONDARY };

                    let rate_str = if is_1x {
                        format!("★ {}", sm::rate_label(rate))
                    } else {
                        format!("  {}", sm::rate_label(rate))
                    };

                    let mut rich_rate = egui::RichText::new(rate_str)
                        .family(egui::FontFamily::Monospace)
                        .color(text_color);
                    if is_1x {
                        rich_rate = rich_rate.strong();
                    }
                    ui.label(rich_rate);

                    for skill in 0..8 {
                        let val = chart.ratings[rate][skill];
                        let mut rich_val = egui::RichText::new(format!("{:.2}", val))
                            .family(egui::FontFamily::Monospace)
                            .color(text_color);
                        if is_1x {
                            rich_val = rich_val.strong();
                        }
                        ui.label(rich_val);
                    }
                    ui.end_row();
                }
            });
    });
}

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let cjk_paths = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
    ];
    for path in cjk_paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "cjk".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "cjk".to_owned());
            fonts
                .families
                .get_mut(&egui::FontFamily::Monospace)
                .unwrap()
                .push("cjk".to_owned());
            break;
        }
    }
    let mono_paths = [
        r"C:\Windows\Fonts\consola.ttf",
        r"C:\Windows\Fonts\CascadiaMono.ttf",
    ];
    for path in mono_paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "mono".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .get_mut(&egui::FontFamily::Monospace)
                .unwrap()
                .insert(0, "mono".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = BG_BASE;
    visuals.panel_fill = BG_BASE;
    visuals.faint_bg_color = BG_CARD;
    visuals.extreme_bg_color = BG_BASE;

    visuals.widgets.noninteractive.bg_fill = BG_CARD;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = BG_CARD;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.hovered.bg_fill = BG_CARD_ALT;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, BORDER_ACTIVE);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.active.bg_fill = ACCENT_BLUE;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);

    visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(0x3B, 0x82, 0xF6, 0x40);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT_BLUE);

    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
    });
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 800.0])
            .with_min_inner_size([780.0, 540.0])
            .with_drag_and_drop(true)
            .with_title("SM5CALC - StepMania 谱面难度分析器"),
        ..Default::default()
    };
    eframe::run_native(
        "SM5CALC",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
