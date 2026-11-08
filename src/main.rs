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

// Skillset Specific Colors (Etterna Community Palette)
const COLOR_OVERALL: egui::Color32 = egui::Color32::from_rgb(0xF5, 0x9E, 0x0B); // Amber #F59E0B
const COLOR_STREAM: egui::Color32 = egui::Color32::from_rgb(0x06, 0xB6, 0xD4);  // Cyan #06B6D4
const COLOR_JS: egui::Color32 = egui::Color32::from_rgb(0x3B, 0x82, 0xF6);      // Blue #3B82F6
const COLOR_HS: egui::Color32 = egui::Color32::from_rgb(0xA8, 0x55, 0xF7);      // Purple #A855F7
const COLOR_STAMINA: egui::Color32 = egui::Color32::from_rgb(0xF9, 0x73, 0x16); // Orange #F97316
const COLOR_JACK: egui::Color32 = egui::Color32::from_rgb(0xEF, 0x44, 0x44);    // Red #EF4444
const COLOR_CJ: egui::Color32 = egui::Color32::from_rgb(0xEA, 0xB3, 0x08);      // Yellow #EAB308
const COLOR_TECH: egui::Color32 = egui::Color32::from_rgb(0x10, 0xB9, 0x81);    // Emerald #10B981

fn skillset_color(index: usize) -> egui::Color32 {
    match index {
        0 => COLOR_OVERALL,
        1 => COLOR_STREAM,
        2 => COLOR_JS,
        3 => COLOR_HS,
        4 => COLOR_STAMINA,
        5 => COLOR_JACK,
        6 => COLOR_CJ,
        7 => COLOR_TECH,
        _ => egui::Color32::WHITE,
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum AnalysisTab {
    Timeline, // NPS 密度时间轴
    Curve,    // 各倍速走势曲线
    Matrix,   // 各倍速矩阵表
}

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
    selected_chart: usize,
    selected_rate: usize, // 0..14, default 3 (1.00x)
    tab: AnalysisTab,
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
            selected_chart: 0,
            selected_rate: 3,
            tab: AnalysisTab::Timeline,
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
                self.selected_chart = 0;
                self.selected_rate = 3;
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

            let selected_chart = self.selected_chart;
            let selected_rate = self.selected_rate;
            let tab = self.tab;

            let updated = self.song.as_ref().map(|song| {
                song_view(ui, song, selected_chart, selected_rate, tab)
            });

            if let Some((new_chart, new_rate, new_tab)) = updated {
                self.selected_chart = new_chart;
                self.selected_rate = new_rate;
                self.tab = new_tab;
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

fn draw_radar_chart(ui: &mut egui::Ui, chart: &Chart, selected_rate: usize) {
    let size = egui::vec2(230.0, 200.0);
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let center = rect.center() + egui::vec2(0.0, 4.0);
    let radius = 68.0f32;
    let axes = 7;
    let labels = sm::skill_labels(chart.key_count);

    let base_values = chart.ratings[3]; // 1.00x
    let active_values = chart.ratings[selected_rate];

    // Find dynamic maximum scale
    let mut max_val = 15.0f32;
    for i in 1..8 {
        max_val = max_val.max(base_values[i]).max(active_values[i]);
    }
    let max_scale = (max_val / 5.0).ceil() * 5.0;

    // Web concentric rings
    let rings = [0.25, 0.50, 0.75, 1.00];
    for &frac in &rings {
        let mut ring_pts = Vec::with_capacity(axes);
        for i in 0..axes {
            let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * (std::f32::consts::TAU / (axes as f32));
            let r = radius * frac;
            ring_pts.push(center + egui::vec2(r * angle.cos(), r * angle.sin()));
        }
        painter.add(egui::Shape::closed_line(
            ring_pts,
            egui::Stroke::new(1.0, BORDER_SUBTLE),
        ));
    }

    // Spoke axes and vertex labels
    for i in 0..axes {
        let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * (std::f32::consts::TAU / (axes as f32));
        let dir = egui::vec2(angle.cos(), angle.sin());
        let end_pt = center + dir * radius;
        painter.line_segment([center, end_pt], egui::Stroke::new(1.0, BORDER_SUBTLE));

        let label_pos = center + dir * (radius + 14.0);
        let color = skillset_color(i + 1);
        painter.text(
            label_pos,
            egui::Align2::CENTER_CENTER,
            labels[i + 1],
            egui::FontId::proportional(11.0),
            color,
        );
    }

    // 1.00x Baseline Polygon (Star Triangles + Outline)
    let mut base_pts = Vec::with_capacity(axes);
    for i in 0..axes {
        let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * (std::f32::consts::TAU / (axes as f32));
        let val = base_values[i + 1];
        let r = (val / max_scale).clamp(0.02, 1.0) * radius;
        base_pts.push(center + egui::vec2(r * angle.cos(), r * angle.sin()));
    }
    let base_fill = if selected_rate == 3 {
        egui::Color32::from_rgba_unmultiplied(0xF5, 0x9E, 0x0B, 0x38)
    } else {
        egui::Color32::from_rgba_unmultiplied(0xF5, 0x9E, 0x0B, 0x18)
    };
    for i in 0..axes {
        let next = (i + 1) % axes;
        painter.add(egui::Shape::convex_polygon(
            vec![center, base_pts[i], base_pts[next]],
            base_fill,
            egui::Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::closed_line(
        base_pts.clone(),
        egui::Stroke::new(1.8, COLOR_OVERALL),
    ));
    for &pt in &base_pts {
        painter.circle_filled(pt, 2.5, COLOR_OVERALL);
    }

    // Active rate polygon (if not 1.00x)
    if selected_rate != 3 {
        let mut active_pts = Vec::with_capacity(axes);
        for i in 0..axes {
            let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * (std::f32::consts::TAU / (axes as f32));
            let val = active_values[i + 1];
            let r = (val / max_scale).clamp(0.02, 1.0) * radius;
            active_pts.push(center + egui::vec2(r * angle.cos(), r * angle.sin()));
        }
        let active_fill = egui::Color32::from_rgba_unmultiplied(0x38, 0xBD, 0xF8, 0x30);
        for i in 0..axes {
            let next = (i + 1) % axes;
            painter.add(egui::Shape::convex_polygon(
                vec![center, active_pts[i], active_pts[next]],
                active_fill,
                egui::Stroke::NONE,
            ));
        }
        painter.add(egui::Shape::closed_line(
            active_pts.clone(),
            egui::Stroke::new(2.0, ACCENT_CYAN),
        ));
        for &pt in &active_pts {
            painter.circle_filled(pt, 3.0, ACCENT_CYAN);
        }
    }
}

fn skill_bars(ui: &mut egui::Ui, chart: &Chart, selected_rate: usize) {
    let labels = sm::skill_labels(chart.key_count);
    let values = chart.ratings[selected_rate];
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
        let color = skillset_color(index);

        ui.horizontal(|ui| {
            ui.add_sized(
                [100.0, 20.0],
                egui::Label::new(
                    egui::RichText::new(labels[index])
                        .color(if is_top { TEXT_PRIMARY } else { TEXT_SECONDARY })
                        .strong()
                        .size(13.0),
                ),
            );
            ui.add_sized(
                [48.0, 20.0],
                egui::Label::new(
                    egui::RichText::new(format!("{:.2}", values[index]))
                        .family(egui::FontFamily::Monospace)
                        .color(if is_top { color } else { TEXT_PRIMARY })
                        .strong()
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
                    .rect_filled(bar, egui::CornerRadius::same(4), color);
            }
            if is_top {
                ui.label(
                    egui::RichText::new("★")
                        .size(11.0)
                        .color(COLOR_OVERALL),
                );
            }
        });
    }
}

fn draw_nps_timeline(ui: &mut egui::Ui, chart: &Chart) {
    if chart.nps_timeline.is_empty() || chart.duration <= 0.05 {
        ui.label(egui::RichText::new("未检测到有效音符时序").color(TEXT_MUTED));
        return;
    }

    let width = ui.available_width();
    let height = 130.0f32;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, egui::CornerRadius::same(8), BG_CARD_ALT);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(8),
        egui::Stroke::new(1.0, BORDER_SUBTLE),
        egui::StrokeKind::Inside,
    );

    let max_y = (chart.peak_nps * 1.15).max(8.0);
    let duration = chart.duration.max(1.0);
    let buckets = &chart.nps_timeline;
    let num_buckets = buckets.len();

    // Horizontal reference grid lines
    let step = if max_y > 40.0 { 20.0 } else if max_y > 20.0 { 10.0 } else { 5.0 };
    let mut ref_val = step;
    while ref_val < max_y {
        let y = rect.bottom() - (ref_val / max_y) * rect.height();
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x10)),
        );
        painter.text(
            egui::pos2(rect.left() + 6.0, y - 2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{:.0}", ref_val),
            egui::FontId::monospace(9.0),
            TEXT_MUTED,
        );
        ref_val += step;
    }

    // Average NPS line
    let avg_y = rect.bottom() - (chart.avg_nps / max_y) * rect.height();
    painter.line_segment(
        [egui::pos2(rect.left(), avg_y), egui::pos2(rect.right(), avg_y)],
        egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(0x38, 0xBD, 0xF8, 0x80)),
    );
    painter.text(
        egui::pos2(rect.right() - 8.0, avg_y - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("平均: {:.1} NPS", chart.avg_nps),
        egui::FontId::proportional(11.0),
        ACCENT_CYAN,
    );

    // Build curve points
    let mut curve_pts = Vec::with_capacity(num_buckets + 2);
    for (i, &nps) in buckets.iter().enumerate() {
        let t = i as f32 * 0.5;
        let x = rect.left() + (t / duration) * rect.width();
        let y = rect.bottom() - (nps / max_y) * rect.height();
        curve_pts.push(egui::pos2(x, y));
    }
    if let Some(&last) = curve_pts.last() {
        curve_pts.push(egui::pos2(rect.right().min(last.x), last.y));
    }

    // Draw trapezoid slices to avoid concave polygon triangulation artifacts
    let area_fill = egui::Color32::from_rgba_unmultiplied(0x38, 0xBD, 0xF8, 0x24);
    if curve_pts.len() >= 2 {
        for w in curve_pts.windows(2) {
            let p0 = egui::pos2(w[0].x, rect.bottom());
            let p1 = w[0];
            let p2 = w[1];
            let p3 = egui::pos2(w[1].x, rect.bottom());
            painter.add(egui::Shape::convex_polygon(
                vec![p0, p1, p2, p3],
                area_fill,
                egui::Stroke::NONE,
            ));
        }
        painter.add(egui::Shape::line(
            curve_pts,
            egui::Stroke::new(2.0, ACCENT_CYAN),
        ));
    }

    // Peak marker
    let peak_idx = buckets
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let peak_t = peak_idx as f32 * 0.5;
    let peak_x = rect.left() + (peak_t / duration) * rect.width();
    let peak_y = rect.bottom() - (chart.peak_nps / max_y) * rect.height();
    painter.circle_filled(egui::pos2(peak_x, peak_y), 4.0, COLOR_OVERALL);
    painter.text(
        egui::pos2(peak_x, peak_y - 6.0),
        egui::Align2::CENTER_BOTTOM,
        format!("★ 峰值 {:.1}", chart.peak_nps),
        egui::FontId::proportional(11.0),
        COLOR_OVERALL,
    );

    // Mouse hover tracking
    if let Some(hover_pos) = response.hover_pos() {
        if rect.contains(hover_pos) {
            let frac = ((hover_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let t = frac * duration;
            let b_idx = ((t / 0.5).floor() as usize).min(num_buckets.saturating_sub(1));
            let current_nps = buckets.get(b_idx).copied().unwrap_or(0.0);

            painter.line_segment(
                [egui::pos2(hover_pos.x, rect.top()), egui::pos2(hover_pos.x, rect.bottom())],
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x90)),
            );

            let dot_y = rect.bottom() - (current_nps / max_y) * rect.height();
            painter.circle_filled(egui::pos2(hover_pos.x, dot_y), 3.5, egui::Color32::WHITE);

            let text = format!("{:.1}s | {:.1} NPS", t, current_nps);
            let text_pos = egui::pos2(
                (hover_pos.x + 8.0).min(rect.right() - 88.0),
                (dot_y - 14.0).max(rect.top() + 8.0),
            );
            painter.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                text,
                egui::FontId::monospace(11.0),
                egui::Color32::WHITE,
            );
        }
    }
}

fn draw_rate_curve(ui: &mut egui::Ui, chart: &Chart, selected_rate: &mut usize) {
    let width = ui.available_width();
    let height = 150.0f32;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, egui::CornerRadius::same(8), BG_CARD_ALT);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(8),
        egui::Stroke::new(1.0, BORDER_SUBTLE),
        egui::StrokeKind::Inside,
    );

    let mut max_msd = 10.0f32;
    for rate_row in &chart.ratings {
        for &val in rate_row {
            max_msd = max_msd.max(val);
        }
    }
    max_msd *= 1.12;

    let x_for_rate = |rate_idx: usize| -> f32 {
        rect.left() + 24.0 + (rate_idx as f32 / 13.0) * (rect.width() - 48.0)
    };
    let y_for_val = |val: f32| -> f32 {
        rect.bottom() - 20.0 - (val / max_msd) * (rect.height() - 34.0)
    };

    // Draw rate columns and labels
    for r in 0..14 {
        let x = x_for_rate(r);
        let is_1x = r == 3;
        let is_selected = r == *selected_rate;

        if is_selected {
            painter.line_segment(
                [egui::pos2(x, rect.top() + 6.0), egui::pos2(x, rect.bottom() - 16.0)],
                egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(0xF5, 0x9E, 0x0B, 0x80)),
            );
        } else {
            painter.line_segment(
                [egui::pos2(x, rect.top() + 6.0), egui::pos2(x, rect.bottom() - 16.0)],
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x0A)),
            );
        }

        let label_color = if is_selected {
            COLOR_OVERALL
        } else if is_1x {
            TEXT_PRIMARY
        } else {
            TEXT_MUTED
        };
        painter.text(
            egui::pos2(x, rect.bottom() - 8.0),
            egui::Align2::CENTER_CENTER,
            sm::rate_label(r),
            egui::FontId::monospace(10.0),
            label_color,
        );
    }

    // 7 Skillset lines (indices 1..8)
    for skill in 1..8 {
        let color = skillset_color(skill);
        let mut pts = Vec::with_capacity(14);
        for r in 0..14 {
            let val = chart.ratings[r][skill];
            pts.push(egui::pos2(x_for_rate(r), y_for_val(val)));
        }
        let alpha = if *selected_rate != 3 { 0x60 } else { 0x90 };
        let stroke_color = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.3, stroke_color)));
    }

    // Overall line (bold, index 0)
    let mut overall_pts = Vec::with_capacity(14);
    for r in 0..14 {
        let val = chart.ratings[r][0];
        overall_pts.push(egui::pos2(x_for_rate(r), y_for_val(val)));
    }
    painter.add(egui::Shape::line(
        overall_pts.clone(),
        egui::Stroke::new(2.5, COLOR_OVERALL),
    ));
    for pt in &overall_pts {
        painter.circle_filled(*pt, 3.0, COLOR_OVERALL);
    }

    // Click to select rate
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut closest_r = 0;
            let mut closest_dist = f32::MAX;
            for r in 0..14 {
                let dist = (x_for_rate(r) - pos.x).abs();
                if dist < closest_dist {
                    closest_dist = dist;
                    closest_r = r;
                }
            }
            *selected_rate = closest_r;
        }
    }
}

fn rate_table(ui: &mut egui::Ui, chart: &Chart, selected_rate: &mut usize) {
    let labels = sm::skill_labels(chart.key_count);
    let table_frame = egui::Frame::default()
        .fill(BG_CARD)
        .stroke(egui::Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12));

    table_frame.show(ui, |ui| {
        egui::Grid::new("rates")
            .striped(true)
            .spacing([14.0, 6.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("音乐速率").strong().color(TEXT_MUTED));
                for (i, label) in labels.iter().enumerate() {
                    ui.label(
                        egui::RichText::new(*label)
                            .strong()
                            .color(skillset_color(i)),
                    );
                }
                ui.end_row();

                for rate in 0..14 {
                    let is_active = rate == *selected_rate;
                    let is_1x = rate == 3;

                    let rate_str = if is_active {
                        format!("► {}", sm::rate_label(rate))
                    } else if is_1x {
                        format!("★ {}", sm::rate_label(rate))
                    } else {
                        format!("  {}", sm::rate_label(rate))
                    };

                    let row_color = if is_active {
                        ACCENT_AMBER
                    } else if is_1x {
                        TEXT_PRIMARY
                    } else {
                        TEXT_SECONDARY
                    };

                    let btn = ui.add(
                        egui::Button::new(
                            egui::RichText::new(rate_str)
                                .family(egui::FontFamily::Monospace)
                                .color(row_color)
                                .strong(),
                        )
                        .fill(if is_active { BG_CARD_ALT } else { egui::Color32::TRANSPARENT })
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(egui::CornerRadius::same(4)),
                    );
                    if btn.clicked() {
                        *selected_rate = rate;
                    }

                    for skill in 0..8 {
                        let val = chart.ratings[rate][skill];
                        let mut rich_val = egui::RichText::new(format!("{:.2}", val))
                            .family(egui::FontFamily::Monospace)
                            .color(row_color);
                        if is_active || is_1x {
                            rich_val = rich_val.strong();
                        }
                        ui.label(rich_val);
                    }
                    ui.end_row();
                }
            });
    });
}

fn song_view(
    ui: &mut egui::Ui,
    song: &Song,
    mut selected_chart: usize,
    mut selected_rate: usize,
    mut tab: AnalysisTab,
) -> (usize, usize, AnalysisTab) {
    if song.charts.is_empty() {
        return (selected_chart, selected_rate, tab);
    }
    selected_chart = selected_chart.min(song.charts.len() - 1);
    selected_rate = selected_rate.min(13);

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
                            let is_current = index == selected_chart;
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
                                selected_chart = index;
                            }
                        }
                    });
                });
            });

            ui.add_space(10.0);
            let chart = &song.charts[selected_chart];

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

            // Core Rating Panel (Overall Score + Radar Chart + Skill Bars)
            let main_card_frame = egui::Frame::default()
                .fill(BG_CARD)
                .stroke(egui::Stroke::new(1.0, BORDER_SUBTLE))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(16.0);

            main_card_frame.show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // Left Column: Overall MSD Display
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        let rate_str = sm::rate_label(selected_rate);
                        ui.label(
                            egui::RichText::new(format!("{rate_str}x 综合评分 (Overall)"))
                                .color(if selected_rate == 3 { TEXT_MUTED } else { ACCENT_CYAN })
                                .size(12.0),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!("{:.2}", chart.ratings[selected_rate][0]))
                                .family(egui::FontFamily::Monospace)
                                .size(54.0)
                                .strong()
                                .color(COLOR_OVERALL),
                        );
                        let meter_line = if chart.meter.is_empty() {
                            "制谱者未标注定数".to_owned()
                        } else {
                            format!("制谱标注定数: {} (仅供参考)", chart.meter)
                        };
                        ui.label(
                            egui::RichText::new(meter_line)
                                .color(TEXT_MUTED)
                                .size(11.0),
                        );
                        if selected_rate != 3 {
                            ui.add_space(4.0);
                            let base_diff = chart.ratings[3][0];
                            let cur_diff = chart.ratings[selected_rate][0];
                            let delta = cur_diff - base_diff;
                            let sign = if delta >= 0.0 { "+" } else { "" };
                            ui.label(
                                egui::RichText::new(format!("对比 1.00x: {sign}{delta:.2}"))
                                    .color(ACCENT_CYAN)
                                    .size(12.0),
                            );
                        }
                    });

                    ui.add_space(12.0);

                    // Center Column: 7-axis Radar Chart
                    ui.vertical(|ui| {
                        draw_radar_chart(ui, chart, selected_rate);
                    });

                    ui.add_space(12.0);

                    // Right Column: Skillset Breakdown Bars
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("7 维能力属性分布 (Skillsets)")
                                .color(TEXT_MUTED)
                                .size(12.0),
                        );
                        ui.add_space(6.0);
                        skill_bars(ui, chart, selected_rate);
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

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

            ui.add_space(14.0);

            // Bottom Section: Tab Switcher
            ui.horizontal(|ui| {
                let tab_btn = |ui: &mut egui::Ui, name: &str, active: bool| -> bool {
                    let (bg, fg) = if active {
                        (ACCENT_BLUE, egui::Color32::WHITE)
                    } else {
                        (BG_CARD_ALT, TEXT_SECONDARY)
                    };
                    ui.add(
                        egui::Button::new(
                            egui::RichText::new(name)
                                .size(13.0)
                                .strong()
                                .color(fg),
                        )
                        .fill(bg)
                        .stroke(if active { egui::Stroke::NONE } else { egui::Stroke::new(1.0, BORDER_SUBTLE) })
                        .corner_radius(egui::CornerRadius::same(6))
                        .min_size(egui::vec2(130.0, 30.0)),
                    )
                    .clicked()
                };

                if tab_btn(ui, "📊 全曲音符密度 (Timeline)", tab == AnalysisTab::Timeline) {
                    tab = AnalysisTab::Timeline;
                }
                if tab_btn(ui, "📈 倍速定数成长曲线 (Rate Curve)", tab == AnalysisTab::Curve) {
                    tab = AnalysisTab::Curve;
                }
                if tab_btn(ui, "📋 完整数据矩阵 (Rate Table)", tab == AnalysisTab::Matrix) {
                    tab = AnalysisTab::Matrix;
                }
            });

            ui.add_space(8.0);

            // Tab Content
            match tab {
                AnalysisTab::Timeline => {
                    draw_nps_timeline(ui, chart);
                }
                AnalysisTab::Curve => {
                    draw_rate_curve(ui, chart, &mut selected_rate);
                }
                AnalysisTab::Matrix => {
                    rate_table(ui, chart, &mut selected_rate);
                }
            }

            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(format!("当前文件: {}", song.path.display()))
                    .color(TEXT_MUTED)
                    .size(11.0),
            );
        });

    (selected_chart, selected_rate, tab)
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
            .with_inner_size([1180.0, 840.0])
            .with_min_inner_size([880.0, 600.0])
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
