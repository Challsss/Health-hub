//! Modern, clean, mobile-first egui UI for Health Hub (Fitbit Air + Xiaomi S400 Scale).
//! Tuned for tall high-DPI screens like the Samsung Galaxy S25 Ultra (camera cutout safe area,
//! gesture-bar bottom padding, large touch targets, and native Android clipboard paste).

use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, Local, NaiveDate};
use eframe::egui::{
    self, Align, Color32, FontId, Frame, Layout, Margin, RichText, Rounding, Sense, Stroke, Ui,
    Vec2,
};
use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints, Points, VLine};

use crate::bodycomp::{self, Rating};
use crate::mibeacon::{self, S400Assembler};
use crate::model::{
    BodyComposition, DayMetrics, MeasurementSource, ScaleMeasurement, Sex, SleepBreakdown,
    UserProfile,
};
use crate::platform::{self, ClipboardTarget, PlatformBridge, PlatformEvent};
use crate::reports::{self, Period, Report, Severity};
use crate::store::Store;

// ---------------------------------------------------------------------------
// Palette & visual tokens (Warm Brown-to-Orange Modern Theme)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Activity,
    Moon,
    Scale,
    Chart,
    Settings,
    Sync,
    Radar,
    Stop,
    Clipboard,
    Clear,
    Trash,
    Key,
    Check,
    Sparkle,
    Sun,
    Calendar,
    Info,
    Warn,
    Alert,
}

#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    surface: Color32,
    surface_alt: Color32,
    border: Color32,
    text: Color32,
    muted: Color32,
    primary: Color32,
    good: Color32,
    warn: Color32,
    alert: Color32,
    info: Color32,
}

impl Palette {
    /// Warm espresso-brown to burnt-orange dark theme.
    fn dark() -> Self {
        Self {
            bg: Color32::from_rgb(20, 14, 10),          // Deep espresso brown
            surface: Color32::from_rgb(34, 24, 18),     // Rich roasted walnut card
            surface_alt: Color32::from_rgb(50, 35, 25), // Warm cocoa surface
            border: Color32::from_rgb(82, 56, 38),      // Bronze / umber border
            text: Color32::from_rgb(252, 243, 234),     // Warm ivory cream
            muted: Color32::from_rgb(188, 156, 130),    // Sandy caramel muted text
            primary: Color32::from_rgb(249, 115, 22),   // Vibrant burnt orange
            good: Color32::from_rgb(245, 158, 11),      // Warm golden amber
            warn: Color32::from_rgb(251, 191, 36),      // Honey gold
            alert: Color32::from_rgb(239, 83, 60),      // Terracotta red-orange
            info: Color32::from_rgb(217, 119, 6),       // Copper amber
        }
    }

    /// Warm latte-cream to burnt-orange light theme.
    fn light() -> Self {
        Self {
            bg: Color32::from_rgb(248, 240, 230),          // Warm sand / latte
            surface: Color32::from_rgb(255, 250, 244),     // Soft ivory card
            surface_alt: Color32::from_rgb(238, 224, 208), // Warm almond
            border: Color32::from_rgb(214, 190, 166),      // Light bronze border
            text: Color32::from_rgb(44, 27, 16),           // Deep espresso text
            muted: Color32::from_rgb(128, 94, 68),         // Roasted coffee muted
            primary: Color32::from_rgb(234, 88, 12),       // Deep burnt orange
            good: Color32::from_rgb(217, 119, 6),          // Rich amber
            warn: Color32::from_rgb(180, 83, 9),           // Warm cinnamon
            alert: Color32::from_rgb(220, 53, 34),         // Terracotta crimson
            info: Color32::from_rgb(194, 65, 12),          // Copper orange
        }
    }

    fn card_frame(self) -> Frame {
        Frame::default()
            .fill(self.surface)
            .stroke(Stroke::new(1.0_f32, self.border))
            .rounding(Rounding::same(18.0))
            .inner_margin(Margin::same(16.0))
    }

    fn card(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
        self.card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
    }

    fn section_header(self, ui: &mut Ui, title: &str, subtitle: Option<&str>) {
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(title)
                    .font(FontId::proportional(18.0))
                    .strong()
                    .color(self.text),
            );
            if let Some(sub) = subtitle {
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).size(13.0).color(self.muted));
                }
            }
        });
    }

    fn progress_bar(ui: &mut Ui, ratio: f32, fill: Color32, bg: Color32) {
        let desired = Vec2::new(ui.available_width(), 9.0);
        let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, Rounding::same(4.5), bg);
        let w = rect.width() * ratio.clamp(0.0, 1.0);
        if w > 1.0 {
            let fill_rect = egui::Rect::from_min_size(rect.min, Vec2::new(w, rect.height()));
            painter.rect_filled(fill_rect, Rounding::same(4.5), fill);
        }
    }

    fn sleep_stage_bar(self, ui: &mut Ui, b: &SleepBreakdown) {
        let total = (b.deep_min + b.rem_min + b.light_min + b.awake_min).max(1.0);
        let desired = Vec2::new(ui.available_width(), 14.0);
        let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, Rounding::same(7.0), self.surface_alt);

        // Warm brown-to-orange sleep stage palette:
        let col_deep = Color32::from_rgb(194, 65, 12);   // Deep: Rich Copper / Rust
        let col_rem = Color32::from_rgb(249, 115, 22);   // REM: Vibrant Burnt Orange
        let col_light = Color32::from_rgb(251, 191, 36); // Light: Warm Amber Gold
        let col_awake = Color32::from_rgb(156, 118, 90); // Awake: Soft Mocha Brown

        let segments = [
            (b.deep_min, col_deep),
            (b.rem_min, col_rem),
            (b.light_min, col_light),
            (b.awake_min, col_awake),
        ];
        let mut x = rect.min.x;
        for (mins, col) in segments {
            let w = (mins / total) as f32 * rect.width();
            if w > 0.5 {
                let seg_rect = egui::Rect::from_min_size(
                    egui::pos2(x, rect.min.y),
                    Vec2::new(w, rect.height()),
                );
                painter.rect_filled(seg_rect, Rounding::same(4.0), col);
                x += w;
            }
        }

        ui.add_space(6.0);
        // Render all 4 sleep stages strictly on a single line (4 equal columns)
        let stages = [
            ("Deep", b.deep_min, col_deep),
            ("REM", b.rem_min, col_rem),
            ("Light", b.light_min, col_light),
            ("Awake", b.awake_min, col_awake),
        ];
        let row_w = ui.available_width();
        let col_w = row_w / 4.0;
        let (row_rect, _) = ui.allocate_exact_size(Vec2::new(row_w, 20.0), Sense::hover());
        for (i, (label, mins, col)) in stages.into_iter().enumerate() {
            let cell_min_x = row_rect.min.x + i as f32 * col_w;
            let dot_center = egui::pos2(cell_min_x + 5.0, row_rect.center().y);
            ui.painter().circle_filled(dot_center, 4.0, col);
            let txt = format!("{label} {:.0}m", mins);
            let galley = ui.painter().layout_no_wrap(
                txt,
                FontId::proportional(12.0),
                self.muted,
            );
            let text_pos = egui::pos2(
                cell_min_x + 13.0,
                row_rect.center().y - galley.size().y * 0.5,
            );
            ui.painter().galley(text_pos, galley, self.muted);
        }
    }

    /// Renders the interactive readout bar directly below a chart showing the selected data point.
    fn chart_readout_bar(self, ui: &mut Ui, date_label: &str, primary_val: &str, details: &str) {
        Frame::default()
            .fill(self.surface_alt)
            .stroke(Stroke::new(1.0_f32, self.primary.gamma_multiply(0.5)))
            .rounding(Rounding::same(12.0))
            .inner_margin(Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                    Self::paint_icon(ui.painter(), Icon::Calendar, icon_rect.center(), 1.0, self.primary);
                    ui.label(
                        RichText::new(date_label)
                            .size(12.5)
                            .strong()
                            .color(self.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(primary_val)
                                .size(14.5)
                                .strong()
                                .color(self.primary),
                        );
                    });
                });
                if !details.is_empty() {
                    ui.label(RichText::new(details).size(12.0).color(self.text));
                }
            });
    }

    /// Universal vector icon painter so all icons render sharply on Android without missing font glyphs.
    fn paint_icon(
        painter: &egui::Painter,
        icon: Icon,
        center: egui::Pos2,
        scale: f32,
        color: Color32,
    ) {
        let s = scale;
        let stroke = Stroke::new(1.6_f32 * s, color);
        let thin = Stroke::new(1.25_f32 * s, color);
        match icon {
            Icon::Home => {
                // Roof
                painter.line_segment(
                    [
                        egui::pos2(center.x - 6.5 * s, center.y - 0.5 * s),
                        egui::pos2(center.x, center.y - 6.5 * s),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x, center.y - 6.5 * s),
                        egui::pos2(center.x + 6.5 * s, center.y - 0.5 * s),
                    ],
                    stroke,
                );
                // Walls
                let body = egui::Rect::from_min_max(
                    egui::pos2(center.x - 4.5 * s, center.y - 1.0 * s),
                    egui::pos2(center.x + 4.5 * s, center.y + 6.0 * s),
                );
                painter.rect_stroke(body, Rounding::same(1.0 * s), stroke);
            }
            Icon::Activity => {
                // ECG pulse polyline
                let pts = vec![
                    egui::pos2(center.x - 7.0 * s, center.y),
                    egui::pos2(center.x - 3.5 * s, center.y),
                    egui::pos2(center.x - 1.5 * s, center.y - 5.5 * s),
                    egui::pos2(center.x + 1.5 * s, center.y + 5.5 * s),
                    egui::pos2(center.x + 3.5 * s, center.y),
                    egui::pos2(center.x + 7.0 * s, center.y),
                ];
                painter.add(egui::Shape::line(pts, stroke));
            }
            Icon::Moon => {
                // Crescent moon: outer circle stroke + inner mask/arc
                painter.circle_stroke(center, 5.5 * s, stroke);
                painter.circle_filled(
                    egui::pos2(center.x + 2.4 * s, center.y - 1.8 * s),
                    4.2 * s,
                    color.gamma_multiply(0.18),
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x + 0.5 * s, center.y - 5.2 * s),
                        egui::pos2(center.x - 1.5 * s, center.y),
                    ],
                    thin,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x - 1.5 * s, center.y),
                        egui::pos2(center.x + 0.5 * s, center.y + 5.2 * s),
                    ],
                    thin,
                );
            }
            Icon::Scale => {
                let outer = egui::Rect::from_min_max(
                    egui::pos2(center.x - 6.0 * s, center.y - 6.0 * s),
                    egui::pos2(center.x + 6.0 * s, center.y + 6.0 * s),
                );
                painter.rect_stroke(outer, Rounding::same(2.5 * s), stroke);
                let dial = egui::Rect::from_min_max(
                    egui::pos2(center.x - 3.2 * s, center.y - 3.8 * s),
                    egui::pos2(center.x + 3.2 * s, center.y - 0.5 * s),
                );
                painter.rect_stroke(dial, Rounding::same(1.0 * s), thin);
                painter.line_segment(
                    [
                        egui::pos2(center.x, center.y - 0.8 * s),
                        egui::pos2(center.x + 1.5 * s, center.y - 2.8 * s),
                    ],
                    thin,
                );
            }
            Icon::Chart => {
                let b1 = egui::Rect::from_min_max(
                    egui::pos2(center.x - 5.5 * s, center.y + 0.5 * s),
                    egui::pos2(center.x - 2.5 * s, center.y + 6.0 * s),
                );
                let b2 = egui::Rect::from_min_max(
                    egui::pos2(center.x - 1.5 * s, center.y - 2.5 * s),
                    egui::pos2(center.x + 1.5 * s, center.y + 6.0 * s),
                );
                let b3 = egui::Rect::from_min_max(
                    egui::pos2(center.x + 2.5 * s, center.y - 5.5 * s),
                    egui::pos2(center.x + 5.5 * s, center.y + 6.0 * s),
                );
                painter.rect_filled(b1, Rounding::same(1.0 * s), color);
                painter.rect_filled(b2, Rounding::same(1.0 * s), color);
                painter.rect_filled(b3, Rounding::same(1.0 * s), color);
            }
            Icon::Settings => {
                // 3 horizontal sliders with knobs
                for (y_off, knob_x) in [(-4.2, -2.0), (0.0, 2.2), (4.2, -1.0)] {
                    let y = center.y + y_off * s;
                    painter.line_segment(
                        [
                            egui::pos2(center.x - 6.0 * s, y),
                            egui::pos2(center.x + 6.0 * s, y),
                        ],
                        thin,
                    );
                    painter.circle_filled(egui::pos2(center.x + knob_x * s, y), 2.1 * s, color);
                }
            }
            Icon::Sync => {
                painter.circle_stroke(center, 5.2 * s, stroke);
                // Arrowhead at top-right
                painter.line_segment(
                    [
                        egui::pos2(center.x + 2.2 * s, center.y - 6.2 * s),
                        egui::pos2(center.x + 5.5 * s, center.y - 3.2 * s),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x + 5.5 * s, center.y - 3.2 * s),
                        egui::pos2(center.x + 1.8 * s, center.y - 2.5 * s),
                    ],
                    stroke,
                );
            }
            Icon::Radar => {
                painter.circle_filled(center, 2.0 * s, color);
                painter.circle_stroke(center, 4.2 * s, thin);
                painter.circle_stroke(center, 6.5 * s, thin);
            }
            Icon::Stop => {
                let r = egui::Rect::from_center_size(center, Vec2::splat(9.0 * s));
                painter.rect_filled(r, Rounding::same(2.0 * s), color);
            }
            Icon::Clipboard => {
                let board = egui::Rect::from_min_max(
                    egui::pos2(center.x - 4.8 * s, center.y - 4.5 * s),
                    egui::pos2(center.x + 4.8 * s, center.y + 6.2 * s),
                );
                painter.rect_stroke(board, Rounding::same(1.5 * s), stroke);
                let clip = egui::Rect::from_min_max(
                    egui::pos2(center.x - 2.5 * s, center.y - 6.2 * s),
                    egui::pos2(center.x + 2.5 * s, center.y - 3.5 * s),
                );
                painter.rect_filled(clip, Rounding::same(1.0 * s), color);
            }
            Icon::Clear => {
                Self::paint_clear_icon(painter, center, color);
            }
            Icon::Trash => {
                Self::paint_trash_icon(painter, center, color);
            }
            Icon::Key => {
                let bow = egui::pos2(center.x - 2.8 * s, center.y);
                painter.circle_stroke(bow, 3.2 * s, stroke);
                painter.line_segment(
                    [
                        egui::pos2(center.x + 0.4 * s, center.y),
                        egui::pos2(center.x + 6.5 * s, center.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x + 3.8 * s, center.y),
                        egui::pos2(center.x + 3.8 * s, center.y + 2.8 * s),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x + 6.0 * s, center.y),
                        egui::pos2(center.x + 6.0 * s, center.y + 3.2 * s),
                    ],
                    stroke,
                );
            }
            Icon::Check => {
                painter.circle_stroke(center, 6.0 * s, thin);
                let pts = vec![
                    egui::pos2(center.x - 3.0 * s, center.y + 0.2 * s),
                    egui::pos2(center.x - 0.8 * s, center.y + 2.6 * s),
                    egui::pos2(center.x + 3.4 * s, center.y - 2.4 * s),
                ];
                painter.add(egui::Shape::line(pts, stroke));
            }
            Icon::Sparkle => {
                painter.line_segment(
                    [
                        egui::pos2(center.x, center.y - 6.0 * s),
                        egui::pos2(center.x, center.y + 6.0 * s),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x - 6.0 * s, center.y),
                        egui::pos2(center.x + 6.0 * s, center.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x - 3.5 * s, center.y - 3.5 * s),
                        egui::pos2(center.x + 3.5 * s, center.y + 3.5 * s),
                    ],
                    thin,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x - 3.5 * s, center.y + 3.5 * s),
                        egui::pos2(center.x + 3.5 * s, center.y - 3.5 * s),
                    ],
                    thin,
                );
            }
            Icon::Sun => {
                painter.circle_filled(center, 3.0 * s, color);
                painter.circle_stroke(center, 5.8 * s, thin);
            }
            Icon::Calendar => {
                let cal = egui::Rect::from_min_max(
                    egui::pos2(center.x - 5.5 * s, center.y - 4.5 * s),
                    egui::pos2(center.x + 5.5 * s, center.y + 5.5 * s),
                );
                painter.rect_stroke(cal, Rounding::same(1.5 * s), stroke);
                painter.line_segment(
                    [
                        egui::pos2(center.x - 5.5 * s, center.y - 1.5 * s),
                        egui::pos2(center.x + 5.5 * s, center.y - 1.5 * s),
                    ],
                    thin,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x - 2.8 * s, center.y - 6.2 * s),
                        egui::pos2(center.x - 2.8 * s, center.y - 3.5 * s),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x + 2.8 * s, center.y - 6.2 * s),
                        egui::pos2(center.x + 2.8 * s, center.y - 3.5 * s),
                    ],
                    stroke,
                );
            }
            Icon::Info => {
                painter.circle_stroke(center, 6.0 * s, thin);
                painter.circle_filled(egui::pos2(center.x, center.y - 2.8 * s), 1.1 * s, color);
                painter.line_segment(
                    [
                        egui::pos2(center.x, center.y - 0.5 * s),
                        egui::pos2(center.x, center.y + 3.6 * s),
                    ],
                    stroke,
                );
            }
            Icon::Warn | Icon::Alert => {
                // Warning triangle with exclamation
                let top = egui::pos2(center.x, center.y - 6.0 * s);
                let bl = egui::pos2(center.x - 6.0 * s, center.y + 5.2 * s);
                let br = egui::pos2(center.x + 6.0 * s, center.y + 5.2 * s);
                painter.add(egui::Shape::closed_line(vec![top, br, bl], stroke));
                painter.line_segment(
                    [
                        egui::pos2(center.x, center.y - 2.0 * s),
                        egui::pos2(center.x, center.y + 1.5 * s),
                    ],
                    stroke,
                );
                painter.circle_filled(egui::pos2(center.x, center.y + 3.5 * s), 1.0 * s, color);
            }
        }
    }

    /// Button with a custom-painted vector icon and text label.
    fn icon_button(
        self,
        ui: &mut Ui,
        icon: Icon,
        label: &str,
        accent: Option<Color32>,
    ) -> egui::Response {
        let fg = accent.unwrap_or(self.text);
        let icon_col = accent.unwrap_or(self.primary);
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            FontId::proportional(13.5),
            fg,
        );
        let size = Vec2::new(galley.size().x + 38.0, 34.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let bg = if resp.is_pointer_button_down_on() || resp.hovered() {
            icon_col.gamma_multiply(0.22)
        } else {
            self.surface_alt
        };
        let border_col = accent
            .map(|c| c.gamma_multiply(0.55))
            .unwrap_or(self.border);
        ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
        ui.painter()
            .rect_stroke(rect, Rounding::same(10.0), Stroke::new(1.0_f32, border_col));
        let icon_center = egui::pos2(rect.min.x + 15.0, rect.center().y);
        Self::paint_icon(ui.painter(), icon, icon_center, 0.95, icon_col);
        let text_pos = egui::pos2(
            rect.min.x + 27.0,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(text_pos, galley, fg);
        resp
    }

    /// Draws a vector trash-can icon centered at `center` with the given `color`.
    fn paint_trash_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
        let stroke = Stroke::new(1.6_f32, color);
        // Lid line
        painter.line_segment(
            [
                egui::pos2(center.x - 6.0, center.y - 4.5),
                egui::pos2(center.x + 6.0, center.y - 4.5),
            ],
            stroke,
        );
        // Handle on top of lid
        painter.line_segment(
            [
                egui::pos2(center.x - 2.5, center.y - 6.5),
                egui::pos2(center.x + 2.5, center.y - 6.5),
            ],
            stroke,
        );
        // Bin body outline
        let body = egui::Rect::from_min_max(
            egui::pos2(center.x - 4.5, center.y - 4.0),
            egui::pos2(center.x + 4.5, center.y + 6.0),
        );
        painter.rect_stroke(body, Rounding::same(1.5), stroke);
        // Vertical ribs inside bin
        let inner_stroke = Stroke::new(1.2_f32, color);
        painter.line_segment(
            [
                egui::pos2(center.x - 1.6, center.y - 1.5),
                egui::pos2(center.x - 1.6, center.y + 3.5),
            ],
            inner_stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x + 1.6, center.y - 1.5),
                egui::pos2(center.x + 1.6, center.y + 3.5),
            ],
            inner_stroke,
        );
    }

    /// Draws a vector circle-X clear icon centered at `center` with the given `color`.
    fn paint_clear_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
        painter.circle_stroke(center, 6.2, Stroke::new(1.4_f32, color));
        let d = 2.7_f32;
        let stroke = Stroke::new(1.6_f32, color);
        painter.line_segment(
            [
                egui::pos2(center.x - d, center.y - d),
                egui::pos2(center.x + d, center.y + d),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x - d, center.y + d),
                egui::pos2(center.x + d, center.y - d),
            ],
            stroke,
        );
    }

    /// Compact icon-only trash button for deleting a weigh-in item.
    fn delete_icon_button(self, ui: &mut Ui) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(32.0, 28.0), Sense::click());
        let bg = if resp.is_pointer_button_down_on() || resp.hovered() {
            self.alert.gamma_multiply(0.25)
        } else {
            self.surface_alt
        };
        let border_col = self.alert.gamma_multiply(0.55);
        ui.painter()
            .rect_filled(rect, Rounding::same(8.0), bg);
        ui.painter()
            .rect_stroke(rect, Rounding::same(8.0), Stroke::new(1.0_f32, border_col));
        Self::paint_trash_icon(ui.painter(), rect.center(), self.alert);
        resp
    }

    /// Button with a vector circle-X clear icon and text label.
    fn clear_button(self, ui: &mut Ui, label: &str) -> egui::Response {
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            FontId::proportional(13.5),
            self.text,
        );
        let size = Vec2::new(galley.size().x + 36.0, 34.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let bg = if resp.is_pointer_button_down_on() || resp.hovered() {
            self.primary.gamma_multiply(0.22)
        } else {
            self.surface_alt
        };
        ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
        ui.painter().rect_stroke(
            rect,
            Rounding::same(10.0),
            Stroke::new(1.0_f32, self.border),
        );
        let icon_center = egui::pos2(rect.min.x + 14.0, rect.center().y);
        Self::paint_clear_icon(ui.painter(), icon_center, self.muted);
        let text_pos = egui::pos2(
            rect.min.x + 25.0,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(text_pos, galley, self.text);
        resp
    }

    /// Button with a vector trash-can icon and alert-colored label.
    fn trash_button(self, ui: &mut Ui, label: &str) -> egui::Response {
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            FontId::proportional(13.5),
            self.alert,
        );
        let size = Vec2::new(galley.size().x + 36.0, 34.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let bg = if resp.is_pointer_button_down_on() || resp.hovered() {
            self.alert.gamma_multiply(0.22)
        } else {
            self.surface_alt
        };
        ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
        ui.painter().rect_stroke(
            rect,
            Rounding::same(10.0),
            Stroke::new(1.0_f32, self.alert.gamma_multiply(0.55)),
        );
        let icon_center = egui::pos2(rect.min.x + 14.0, rect.center().y);
        Self::paint_trash_icon(ui.painter(), icon_center, self.alert);
        let text_pos = egui::pos2(
            rect.min.x + 25.0,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(text_pos, galley, self.alert);
        resp
    }

    #[allow(dead_code)]
    fn rating_color(self, r: Rating) -> Color32 {
        match r {
            Rating::Healthy => self.good,
            Rating::Low | Rating::High => self.warn,
            Rating::VeryHigh => self.alert,
        }
    }

    fn recommendation_card(self, ui: &mut Ui, r: &reports::Recommendation) {
        let (accent, icon) = match r.severity {
            Severity::Alert => (self.alert, Icon::Alert),
            Severity::Warning => (self.warn, Icon::Warn),
            Severity::Info => (self.info, Icon::Info),
            Severity::Good => (self.good, Icon::Check),
        };
        Frame::default()
            .fill(self.surface)
            .stroke(Stroke::new(1.2_f32, accent.gamma_multiply(0.6)))
            .rounding(Rounding::same(14.0))
            .inner_margin(Margin::same(14.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                    Self::paint_icon(ui.painter(), icon, icon_rect.center(), 1.0, accent);
                    ui.label(
                        RichText::new(r.category.label())
                            .size(12.5)
                            .strong()
                            .color(accent),
                    );
                    ui.label(
                        RichText::new(&r.title)
                            .size(15.5)
                            .strong()
                            .color(self.text),
                    );
                });
                ui.add_space(4.0);
                ui.label(RichText::new(&r.detail).size(13.5).color(self.muted));
            });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Today,
    Activity,
    Sleep,
    Body,
    Reports,
    Settings,
}

pub struct HealthHubApp {
    pub store: Store,
    pub bridge: PlatformBridge,
    pub assembler: S400Assembler,
    pub tab: Tab,
    pub dark_mode: bool,
    pub status_msg: String,
    pub health_granted: bool,
    pub ble_granted: bool,
    pub hc_available: bool,
    pub sync_days_back: i64,
    pub last_ble_poll: Instant,
    pub last_seen_mac: Option<(String, i32)>,

    /// Top & bottom safe-area insets in logical points (for S25 Ultra camera cutout & gesture bar).
    pub safe_top: f32,
    pub safe_bottom: f32,

    // Manual weigh-in inputs
    pub manual_weight: String,
    pub manual_impedance: String,
    pub manual_body_fat: String,

    // Settings form inputs
    pub form_height_cm: String,
    pub form_birth_year: String,
    pub form_sex: Sex,
    pub form_bindkey: String,
    pub form_profile_slot: String,
    pub form_scale_mac: String,

    // Reports state
    pub report_period: Period,
    pub cached_report: Option<(Period, NaiveDate, Report)>,

    // Interactive chart selection state (readout shown below each chart)
    pub selected_steps_idx: Option<usize>,
    pub selected_heart_idx: Option<usize>,
    pub selected_sleep_idx: Option<usize>,
    pub selected_body_idx: Option<usize>,
    pub heart_show_hrv: bool,
    pub body_show_bf: bool,
}

impl HealthHubApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let path = PlatformBridge::storage_path();
        let store = Store::load(&path);
        let profile = store.profile();

        let bridge = PlatformBridge::new();
        bridge.check_permissions();
        bridge.query_safe_insets();

        // Default safe insets: generous padding on Android so the camera notch and
        // bottom gesture bar are cleared immediately on frame 1.
        let (default_top, default_bottom) = if cfg!(target_os = "android") {
            (48.0, 28.0)
        } else {
            (10.0, 6.0)
        };

        let app = Self {
            form_height_cm: format!("{:.0}", profile.height_cm),
            form_birth_year: profile.birth_year.to_string(),
            form_sex: profile.sex,
            form_bindkey: store.settings.bindkey.clone(),
            form_profile_slot: store
                .settings
                .scale_profile_id
                .map(|v| v.to_string())
                .unwrap_or_default(),
            form_scale_mac: store.settings.scale_mac.clone().unwrap_or_default(),
            store,
            bridge,
            assembler: S400Assembler::default(),
            tab: Tab::Today,
            dark_mode: true,
            status_msg: String::new(),
            health_granted: false,
            ble_granted: false,
            hc_available: true,
            sync_days_back: 30,
            last_ble_poll: Instant::now(),
            last_seen_mac: None,
            safe_top: default_top,
            safe_bottom: default_bottom,
            manual_weight: "75.0".into(),
            manual_impedance: String::new(),
            manual_body_fat: String::new(),
            report_period: Period::Week,
            cached_report: None,
            selected_steps_idx: None,
            selected_heart_idx: None,
            selected_sleep_idx: None,
            selected_body_idx: None,
            heart_show_hrv: false,
            body_show_bf: false,
        };
        app.apply_theme(&cc.egui_ctx);
        app
    }

    fn pal(&self) -> Palette {
        if self.dark_mode {
            Palette::dark()
        } else {
            Palette::light()
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let p = self.pal();
        let mut visuals = if self.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = p.bg;
        visuals.window_fill = p.surface;
        visuals.extreme_bg_color = p.surface_alt;
        visuals.faint_bg_color = p.surface_alt;
        visuals.override_text_color = Some(p.text);
        visuals.widgets.noninteractive.bg_fill = p.surface;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
        visuals.widgets.noninteractive.rounding = Rounding::same(12.0);
        visuals.widgets.inactive.bg_fill = p.surface_alt;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
        visuals.widgets.inactive.rounding = Rounding::same(10.0);
        visuals.widgets.hovered.bg_fill = p.primary.gamma_multiply(0.25);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.primary);
        visuals.widgets.hovered.rounding = Rounding::same(10.0);
        visuals.widgets.active.bg_fill = p.primary.gamma_multiply(0.35);
        visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, p.primary);
        visuals.widgets.active.rounding = Rounding::same(10.0);
        visuals.selection.bg_fill = p.primary.gamma_multiply(0.28);
        visuals.selection.stroke = Stroke::new(1.5_f32, p.primary);
        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(12.0, 12.0);
        style.spacing.button_padding = egui::vec2(14.0, 10.0);
        ctx.set_style(style);
    }

    fn invalidate_report(&mut self) {
        self.cached_report = None;
    }

    fn save_store(&mut self) {
        self.invalidate_report();
        if let Err(e) = self.store.save() {
            self.status_msg = format!("Save error: {e}");
        }
    }

    fn process_events(&mut self) {
        let now_ms = Local::now().timestamp_millis();
        if self.bridge.scanning && self.last_ble_poll.elapsed() >= StdDuration::from_millis(750) {
            self.last_ble_poll = Instant::now();
            self.bridge.poll_ble_packets();
            if let Some(m) = self.assembler.flush_if_stale(now_ms) {
                self.commit_s400_measurement(m);
            }
        }

        for ev in self.bridge.poll() {
            match ev {
                PlatformEvent::Status(msg) => {
                    self.status_msg = msg;
                }
                PlatformEvent::PermissionsStatus {
                    health_granted,
                    ble_granted,
                    hc_available,
                } => {
                    self.health_granted = health_granted;
                    self.ble_granted = ble_granted;
                    self.hc_available = hc_available;
                }
                PlatformEvent::SafeInsets { top, bottom } => {
                    self.safe_top = top.max(44.0);
                    self.safe_bottom = bottom.max(24.0);
                }
                PlatformEvent::ClipboardPasted { target, text } => {
                    let cleaned = text.trim();
                    if cleaned.is_empty() {
                        self.status_msg = "Clipboard is empty.".into();
                    } else {
                        match target {
                            ClipboardTarget::Bindkey => {
                                // Keep only hex characters in case the user copied surrounding spaces/quotes
                                let hex_only: String =
                                    cleaned.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                                let final_key = if hex_only.len() == 32 {
                                    hex_only
                                } else {
                                    cleaned.to_string()
                                };
                                self.form_bindkey = final_key;
                                self.status_msg = format!(
                                    "Pasted bindkey ({} chars). Tap 'Save Settings' below.",
                                    self.form_bindkey.len()
                                );
                            }
                            ClipboardTarget::ScaleMac => {
                                self.form_scale_mac = cleaned.to_string();
                                self.status_msg =
                                    "Pasted scale MAC address. Tap 'Save Settings' below.".into();
                            }
                        }
                    }
                }
                PlatformEvent::HealthData(Ok(import)) => {
                    let summary = self.store.merge_health(import);
                    self.store.last_sync_ms = Some(now_ms);
                    self.save_store();
                    self.status_msg = summary;
                }
                PlatformEvent::HealthData(Err(e)) => {
                    self.status_msg = format!("Health Connect: {e}");
                }
                PlatformEvent::BleAdvertisements(ads) => {
                    let key = mibeacon::parse_bindkey(&self.store.settings.bindkey);
                    for ad in ads {
                        if let Some(filter_mac) = &self.store.settings.scale_mac {
                            if !filter_mac.trim().is_empty()
                                && !filter_mac.eq_ignore_ascii_case(&ad.mac)
                            {
                                continue;
                            }
                        }
                        self.last_seen_mac = Some((ad.mac.clone(), ad.rssi));
                        let Some(mac_bytes) = mibeacon::parse_mac(&ad.mac) else {
                            continue;
                        };
                        let Some(raw_bytes) = mibeacon::parse_hex(&ad.data_hex) else {
                            continue;
                        };
                        if let Some(m) =
                            self.assembler
                                .feed(mac_bytes, &raw_bytes, key.as_ref(), ad.time_ms)
                        {
                            if self.store.settings.scale_mac.is_none() {
                                self.store.settings.scale_mac = Some(ad.mac.clone());
                                self.form_scale_mac = ad.mac.clone();
                            }
                            self.commit_s400_measurement(m);
                        } else if self.assembler.has_pending() {
                            self.status_msg = format!(
                                "Weight locked ({}) — keep standing barefoot for impedance…",
                                ad.mac
                            );
                        } else if let Some(err) = &self.assembler.last_error {
                            if self.assembler.decrypt_failures > 0 {
                                self.status_msg = format!(
                                    "S400 decrypt failed ({err}) — check bindkey in Settings"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn commit_s400_measurement(&mut self, m: mibeacon::S400Measurement) {
        if let Some(slot) = self.store.settings.scale_profile_id {
            if slot != 0 && m.profile_id != slot {
                self.status_msg = format!(
                    "Ignored weigh-in for profile #{} (filtered to #{slot})",
                    m.profile_id
                );
                return;
            }
        }
        let meas = ScaleMeasurement {
            time_ms: m.time_ms,
            weight_kg: m.weight_kg,
            impedance_low: m.impedance_low,
            impedance_high: m.impedance_high,
            heart_rate: m.heart_rate,
            profile_id: Some(m.profile_id),
            reported_body_fat: None,
            source: MeasurementSource::ScaleBle,
        };
        self.store.add_measurement(meas);
        self.save_store();
        self.status_msg = format!(
            "Saved S400 weigh-in: {:.1} kg{}",
            m.weight_kg,
            m.heart_rate
                .map(|hr| format!(" • {hr} bpm"))
                .unwrap_or_default()
        );
    }

    // -----------------------------------------------------------------------
    // Top Header (with camera notch safe padding) & Bottom Navigation
    // -----------------------------------------------------------------------

    fn ui_header(&mut self, ui: &mut Ui) {
        let p = self.pal();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Health Hub")
                    .font(FontId::proportional(22.0))
                    .strong()
                    .color(p.text),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let settings_active = self.tab == Tab::Settings;
                let settings_accent = if settings_active { Some(p.primary) } else { None };
                if p.icon_button(ui, Icon::Settings, "Settings", settings_accent)
                    .clicked()
                {
                    self.bridge.check_permissions();
                    self.tab = Tab::Settings;
                }

                if p.icon_button(ui, Icon::Sync, "Sync", None)
                    .on_hover_text("Sync Fitbit Air metrics from Health Connect")
                    .clicked()
                {
                    self.bridge.sync_health_connect(self.sync_days_back);
                }
            });
        });

        if !self.status_msg.is_empty() {
            ui.add_space(2.0);
            ui.label(RichText::new(&self.status_msg).size(12.5).color(p.primary));
        }
    }

    fn ui_bottom_nav(&mut self, ui: &mut Ui) {
        let p = self.pal();
        let items = [
            (Tab::Today, Icon::Home, "Today"),
            (Tab::Activity, Icon::Activity, "Activity"),
            (Tab::Sleep, Icon::Moon, "Sleep"),
            (Tab::Body, Icon::Scale, "Scale"),
            (Tab::Reports, Icon::Chart, "Reports"),
        ];
        let cols = items.len();
        let avail_w = ui.available_width();
        let col_w = (avail_w / cols as f32).max(56.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, icon, label) in items {
                let selected = self.tab == t;
                let fg = if selected { p.primary } else { p.muted };
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(col_w, 60.0), Sense::click());
                if selected {
                    let pill = rect.shrink2(Vec2::new(6.0, 4.0));
                    ui.painter().rect_filled(
                        pill,
                        Rounding::same(14.0),
                        p.primary.gamma_multiply(0.18),
                    );
                }
                let icon_center = egui::pos2(rect.center().x, rect.min.y + 18.0);
                Palette::paint_icon(ui.painter(), icon, icon_center, 1.15, fg);

                let label_galley = ui.painter().layout_no_wrap(
                    label.to_string(),
                    FontId::proportional(13.0),
                    fg,
                );
                let label_pos = egui::pos2(
                    rect.center().x - label_galley.size().x * 0.5,
                    rect.min.y + 35.0,
                );
                ui.painter().galley(label_pos, label_galley, fg);

                if resp.clicked() {
                    self.tab = t;
                }
            }
        });
    }

    // -----------------------------------------------------------------------
    // Today Tab (Clean, Uncluttered Hero Cards)
    // -----------------------------------------------------------------------

    fn ui_today(&mut self, ui: &mut Ui) {
        let p = self.pal();
        let today = Local::now().date_naive();
        let yesterday = today - Duration::days(1);

        // Onboarding card when empty
        if self.store.days.is_empty() && self.store.scale.is_empty() {
            p.card(ui, |ui| {
                ui.label(
                    RichText::new("Welcome to Health Hub")
                        .size(18.0)
                        .strong()
                        .color(p.primary),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Bring your Google Fitbit Air metrics and Xiaomi Scale S400 measurements together in one place.",
                    )
                    .size(14.0)
                    .color(p.muted),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if p.icon_button(ui, Icon::Key, "1. Grant Permissions", Some(p.primary))
                        .clicked()
                    {
                        self.bridge.request_permissions();
                    }
                    if p.icon_button(ui, Icon::Sync, "2. Sync Fitbit Air", None)
                        .clicked()
                    {
                        self.bridge.sync_health_connect(self.sync_days_back);
                    }
                    if p.icon_button(ui, Icon::Sparkle, "Load Demo Data", None)
                        .clicked()
                    {
                        platform::populate_demo_data(&mut self.store);
                        self.save_store();
                        self.status_msg = "Loaded 35 days of demo data.".into();
                    }
                });
            });
        }

        // 1. Fitbit Air Hero Card
        let day = self
            .store
            .days
            .get(&today)
            .or_else(|| self.store.days.get(&yesterday))
            .cloned()
            .unwrap_or_else(|| DayMetrics::new(today));

        let steps_val = day.steps.unwrap_or(0);
        let steps_ratio = steps_val as f32 / 10_000.0;
        let steps_col = if steps_val >= 10_000 {
            p.good
        } else if steps_val >= 7_500 {
            p.primary
        } else {
            p.warn
        };

        p.card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("FITBIT AIR • ACTIVITY")
                        .size(12.0)
                        .strong()
                        .color(p.muted),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(day.date.to_string())
                            .size(12.0)
                            .color(p.muted),
                    );
                });
            });

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(
                        day.steps
                            .map(Self::fmt_thousands)
                            .unwrap_or_else(|| "–".into()),
                    )
                    .font(FontId::proportional(30.0))
                    .strong()
                    .color(p.text),
                );
                ui.label(
                    RichText::new(format!(
                        "/ 10,000 steps{}",
                        day.distance_m
                            .map(|m| format!(" • {:.2} km", m / 1000.0))
                            .unwrap_or_default()
                    ))
                    .size(14.0)
                    .color(p.muted),
                );
            });

            ui.add_space(4.0);
            Palette::progress_bar(ui, steps_ratio, steps_col, p.surface_alt);
            ui.add_space(8.0);

            // Clean 2x2 summary inside the card
            let col_w = (ui.available_width() - 16.0) * 0.5;
            egui::Grid::new("today_vitals_subgrid")
                .num_columns(2)
                .min_col_width(col_w)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    Self::mini_stat(
                        ui,
                        p,
                        "Active Calories",
                        &day.active_kcal
                            .map(|v| format!("{v:.0} kcal"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    Self::mini_stat(
                        ui,
                        p,
                        "Exercise",
                        &day.exercise_min
                            .map(|v| format!("{v:.0} min"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    ui.end_row();

                    Self::mini_stat(
                        ui,
                        p,
                        "Resting HR",
                        &day.resting_hr
                            .map(|v| format!("{v:.0} bpm"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    Self::mini_stat(
                        ui,
                        p,
                        "HRV (Recovery)",
                        &day.hrv_rmssd
                            .map(|v| format!("{v:.0} ms"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    ui.end_row();
                });
        });

        // 2. Sleep Hero Card
        let sleep_session = self
            .store
            .sleep_for(today)
            .or_else(|| self.store.sleep_for(yesterday))
            .cloned();
        p.card(ui, |ui| {
            ui.label(
                RichText::new("FITBIT AIR • SLEEP")
                    .size(12.0)
                    .strong()
                    .color(p.muted),
            );
            if let Some(s) = sleep_session {
                let b = s.breakdown();
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(reports::fmt_hours(b.asleep_min))
                            .font(FontId::proportional(26.0))
                            .strong()
                            .color(p.text),
                    );
                    ui.label(
                        RichText::new(format!(
                            "asleep • Bedtime {}",
                            reports::fmt_clock(s.bedtime_min as f64)
                        ))
                        .size(13.5)
                        .color(p.muted),
                    );
                });
                ui.add_space(6.0);
                p.sleep_stage_bar(ui, &b);
            } else {
                ui.label(
                    RichText::new("No sleep session recorded yet.")
                        .size(14.0)
                        .color(p.muted),
                );
            }
        });

        // 3. Xiaomi S400 Scale Hero Card
        let profile = self.store.profile();
        let age = profile.age_on(today);
        let latest_meas = self.store.latest_measurement().cloned();

        p.card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("XIAOMI S400 • BODY COMPOSITION")
                        .size(12.0)
                        .strong()
                        .color(p.muted),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let resp = if self.bridge.scanning {
                        p.icon_button(ui, Icon::Radar, "Scanning…", Some(p.good))
                    } else {
                        p.icon_button(ui, Icon::Radar, "Scan Scale", None)
                    };
                    if resp.clicked() {
                        let next = !self.bridge.scanning;
                        self.bridge.set_ble_scanning(next);
                    }
                });
            });

            if let Some(m) = latest_meas {
                let c = bodycomp::compute(&m, &profile, age);
                ui.add_space(4.0);
                let col_w = (ui.available_width() - 16.0) * 0.5;
                egui::Grid::new("today_body_subgrid")
                    .num_columns(2)
                    .min_col_width(col_w)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        Self::mini_stat(
                            ui,
                            p,
                            "Weight",
                            &format!("{:.1} kg", c.weight_kg),
                        );
                        Self::mini_stat(
                            ui,
                            p,
                            "Body Fat",
                            &c.body_fat_pct
                                .map(|v| format!("{v:.1} %"))
                                .unwrap_or_else(|| "–".into()),
                        );
                        ui.end_row();

                        Self::mini_stat(
                            ui,
                            p,
                            "Muscle Mass",
                            &c.muscle_mass_kg
                                .map(|v| format!("{v:.1} kg"))
                                .unwrap_or_else(|| "–".into()),
                        );
                        Self::mini_stat(ui, p, "BMI", &format!("{:.1}", c.bmi));
                        ui.end_row();
                    });
            } else {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("No weigh-ins yet. Step on your S400 barefoot while scanning.")
                        .size(13.5)
                        .color(p.muted),
                );
            }
        });

        // 4. Top 2 Recommendations
        let report = self.get_report(Period::Week);
        if !report.recommendations.is_empty() {
            p.section_header(ui, "Top Insights", Some("• Last 7 days"));
            for r in report.recommendations.iter().take(2) {
                p.recommendation_card(ui, r);
            }
            if ui
                .button(
                    RichText::new(format!(
                        "See all {} insights in Reports →",
                        report.recommendations.len()
                    ))
                    .size(13.5)
                    .color(p.primary),
                )
                .clicked()
            {
                self.tab = Tab::Reports;
            }
        }
    }

    fn mini_stat(ui: &mut Ui, p: Palette, label: &str, value: &str) {
        ui.vertical(|ui| {
            ui.label(RichText::new(label).size(12.0).color(p.muted));
            ui.label(
                RichText::new(value)
                    .font(FontId::proportional(19.0))
                    .strong()
                    .color(p.text),
            );
        });
    }

    /// Updates a chart's selected point index when the user touches or slides their finger on the plot.
    fn update_chart_touch_selection(
        ui: &Ui,
        plot_resp: &egui_plot::PlotResponse<()>,
        len: usize,
        selected: &mut Option<usize>,
    ) -> usize {
        if len == 0 {
            return 0;
        }
        let touching = ui.ctx().input(|i| i.pointer.any_down() || i.pointer.any_pressed());
        if touching {
            if let Some(pos) = ui.ctx().pointer_latest_pos() {
                if plot_resp.response.rect.contains(pos) {
                    let pt = plot_resp.transform.value_from_position(pos);
                    let idx = pt.x.round().clamp(0.0, (len - 1) as f64) as usize;
                    if *selected != Some(idx) {
                        *selected = Some(idx);
                        ui.ctx().request_repaint();
                    }
                }
            }
        }
        selected
            .map(|i| i.min(len - 1))
            .unwrap_or(len - 1)
    }

    // -----------------------------------------------------------------------
    // Activity Tab
    // -----------------------------------------------------------------------

    fn ui_activity(&mut self, ui: &mut Ui) {
        let p = self.pal();
        p.section_header(ui, "Activity & Heart", Some("• Google Fitbit Air"));

        if self.store.days.is_empty() {
            p.card(ui, |ui| {
                ui.label(
                    RichText::new("No daily Fitbit Air data synced yet.").color(p.muted),
                );
            });
            return;
        }

        let days: Vec<_> = self.store.days.values().cloned().collect();
        let steps_slice: Vec<_> = days
            .iter()
            .rev()
            .take(14)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        p.card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Daily Steps (Last 14 Days)").size(15.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Touch chart to inspect").size(11.5).color(p.muted));
                });
            });
            let sel_steps = self
                .selected_steps_idx
                .map(|i| i.min(steps_slice.len().saturating_sub(1)))
                .unwrap_or_else(|| steps_slice.len().saturating_sub(1));

            let bars: Vec<Bar> = steps_slice
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let s = d.steps.unwrap_or(0) as f64;
                    let is_sel = i == sel_steps;
                    let base_col = if s >= 10_000.0 {
                        p.primary
                    } else if s >= 7_500.0 {
                        p.good
                    } else {
                        p.muted.gamma_multiply(0.75)
                    };
                    let col = if is_sel {
                        p.warn
                    } else {
                        base_col
                    };
                    Bar::new(i as f64, s).width(0.62).fill(col)
                })
                .collect();

            let plot_resp = Plot::new("steps_plot")
                .height(165.0)
                .allow_zoom(false)
                .allow_drag(false)
                .allow_scroll(false)
                .show_x(false)
                .show_y(false)
                .show_grid([false, true])
                .show_axes([false, true])
                .label_formatter(|_, _| String::new())
                .show(ui, |plot_ui| {
                    plot_ui.bar_chart(BarChart::new(bars));
                    plot_ui.vline(
                        VLine::new(sel_steps as f64)
                            .color(p.primary.gamma_multiply(0.45))
                            .width(1.5_f32),
                    );
                });

            let active_idx = Self::update_chart_touch_selection(
                ui,
                &plot_resp,
                steps_slice.len(),
                &mut self.selected_steps_idx,
            );
            if let Some(d) = steps_slice.get(active_idx) {
                let steps_str = format!(
                    "{} steps",
                    d.steps
                        .map(Self::fmt_thousands)
                        .unwrap_or_else(|| "0".into())
                );
                let details = format!(
                    "Distance {} • Active {} • Exercise {}",
                    d.distance_m
                        .map(|m| format!("{:.2} km", m / 1000.0))
                        .unwrap_or_else(|| "–".into()),
                    d.active_kcal
                        .map(|k| format!("{k:.0} kcal"))
                        .unwrap_or_else(|| "–".into()),
                    d.exercise_min
                        .map(|m| format!("{m:.0} min"))
                        .unwrap_or_else(|| "–".into()),
                );
                ui.add_space(6.0);
                p.chart_readout_bar(ui, &d.date.to_string(), &steps_str, &details);
            }
        });

        let heart_slice: Vec<_> = days
            .iter()
            .filter(|d| d.resting_hr.is_some() || d.hrv_rmssd.is_some())
            .rev()
            .take(21)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        p.card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Heart & Recovery").size(15.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .selectable_label(self.heart_show_hrv, "HRV (ms)")
                        .clicked()
                    {
                        self.heart_show_hrv = true;
                    }
                    if ui
                        .selectable_label(!self.heart_show_hrv, "Resting HR (bpm)")
                        .clicked()
                    {
                        self.heart_show_hrv = false;
                    }
                });
            });

            if heart_slice.is_empty() {
                ui.label(
                    RichText::new("No resting heart rate or HRV records synced yet.")
                        .size(13.0)
                        .color(p.muted),
                );
            } else {
                let sel_heart = self
                    .selected_heart_idx
                    .map(|i| i.min(heart_slice.len() - 1))
                    .unwrap_or(heart_slice.len() - 1);

                let pts_vec: Vec<[f64; 2]> = heart_slice
                    .iter()
                    .enumerate()
                    .filter_map(|(i, d)| {
                        let val = if self.heart_show_hrv {
                            d.hrv_rmssd.or(d.resting_hr)
                        } else {
                            d.resting_hr.or(d.hrv_rmssd)
                        };
                        val.map(|v| [i as f64, v])
                    })
                    .collect();

                let sel_pt: Vec<[f64; 2]> = pts_vec
                    .iter()
                    .copied()
                    .filter(|pt| (pt[0] - sel_heart as f64).abs() < 0.5)
                    .collect();

                let line_col = if self.heart_show_hrv {
                    p.good
                } else {
                    p.primary
                };

                let plot_resp = Plot::new("heart_plot")
                    .height(165.0)
                    .allow_zoom(false)
                    .allow_drag(false)
                    .allow_scroll(false)
                    .show_x(false)
                    .show_y(false)
                    .show_grid([false, true])
                    .show_axes([false, true])
                    .label_formatter(|_, _| String::new())
                    .show(ui, |plot_ui| {
                        plot_ui.line(
                            Line::new(PlotPoints::from(pts_vec.clone()))
                                .color(line_col)
                                .width(2.5_f32),
                        );
                        plot_ui.points(
                            Points::new(PlotPoints::from(pts_vec))
                                .color(line_col)
                                .radius(3.5_f32),
                        );
                        plot_ui.vline(
                            VLine::new(sel_heart as f64)
                                .color(p.warn.gamma_multiply(0.6))
                                .width(1.5_f32),
                        );
                        if !sel_pt.is_empty() {
                            plot_ui.points(
                                Points::new(PlotPoints::from(sel_pt))
                                    .color(p.warn)
                                    .radius(6.0_f32),
                            );
                        }
                    });

                let active_idx = Self::update_chart_touch_selection(
                    ui,
                    &plot_resp,
                    heart_slice.len(),
                    &mut self.selected_heart_idx,
                );
                if let Some(d) = heart_slice.get(active_idx) {
                    let primary = if self.heart_show_hrv {
                        d.hrv_rmssd
                            .map(|v| format!("HRV {v:.0} ms"))
                            .unwrap_or_else(|| "HRV –".into())
                    } else {
                        d.resting_hr
                            .map(|v| format!("RHR {v:.0} bpm"))
                            .unwrap_or_else(|| "RHR –".into())
                    };
                    let details = format!(
                        "Resting HR: {} • HRV (RMSSD): {}",
                        d.resting_hr
                            .map(|v| format!("{v:.0} bpm"))
                            .unwrap_or_else(|| "–".into()),
                        d.hrv_rmssd
                            .map(|v| format!("{v:.0} ms"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    ui.add_space(6.0);
                    p.chart_readout_bar(ui, &d.date.to_string(), &primary, &details);
                }
            }
        });

        p.section_header(ui, "Recent Days", None);
        for d in days.iter().rev().take(14) {
            p.card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(d.date.to_string()).size(15.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} steps",
                                d.steps
                                    .map(Self::fmt_thousands)
                                    .unwrap_or_else(|| "–".into())
                            ))
                            .size(16.0)
                            .strong()
                            .color(p.primary),
                        );
                    });
                });
                ui.label(
                    RichText::new(format!(
                        "{} • Active {} • Exercise {} • RHR {} • HRV {}",
                        d.distance_m
                            .map(|v| format!("{:.1} km", v / 1000.0))
                            .unwrap_or_else(|| "–".into()),
                        d.active_kcal
                            .map(|v| format!("{v:.0} kcal"))
                            .unwrap_or_else(|| "–".into()),
                        d.exercise_min
                            .map(|v| format!("{v:.0}m"))
                            .unwrap_or_else(|| "–".into()),
                        d.resting_hr
                            .map(|v| format!("{v:.0} bpm"))
                            .unwrap_or_else(|| "–".into()),
                        d.hrv_rmssd
                            .map(|v| format!("{v:.0} ms"))
                            .unwrap_or_else(|| "–".into()),
                    ))
                    .size(12.5)
                    .color(p.muted),
                );
            });
        }
    }

    // -----------------------------------------------------------------------
    // Sleep Tab
    // -----------------------------------------------------------------------

    fn ui_sleep(&mut self, ui: &mut Ui) {
        let p = self.pal();
        p.section_header(ui, "Sleep & Recovery", Some("• Google Fitbit Air"));

        if self.store.sleep.is_empty() {
            p.card(ui, |ui| {
                ui.label(RichText::new("No sleep sessions synced yet.").color(p.muted));
            });
            return;
        }

        let recent: Vec<_> = self
            .store
            .sleep
            .iter()
            .rev()
            .take(14)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        p.card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sleep Duration (Last 14 Nights)").size(15.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Touch chart to inspect").size(11.5).color(p.muted));
                });
            });
            let sel_sleep = self
                .selected_sleep_idx
                .map(|i| i.min(recent.len().saturating_sub(1)))
                .unwrap_or_else(|| recent.len().saturating_sub(1));

            let bars: Vec<Bar> = recent
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let hrs = s.breakdown().asleep_min / 60.0;
                    let is_sel = i == sel_sleep;
                    let base_col = if (7.0..=9.0).contains(&hrs) {
                        p.primary
                    } else if hrs >= 6.0 {
                        p.good
                    } else {
                        p.muted.gamma_multiply(0.75)
                    };
                    let col = if is_sel { p.warn } else { base_col };
                    Bar::new(i as f64, hrs).width(0.62).fill(col)
                })
                .collect();

            let plot_resp = Plot::new("sleep_plot")
                .height(165.0)
                .allow_zoom(false)
                .allow_drag(false)
                .allow_scroll(false)
                .show_x(false)
                .show_y(false)
                .show_grid([false, true])
                .show_axes([false, true])
                .label_formatter(|_, _| String::new())
                .show(ui, |plot_ui| {
                    plot_ui.bar_chart(BarChart::new(bars));
                    plot_ui.vline(
                        VLine::new(sel_sleep as f64)
                            .color(p.primary.gamma_multiply(0.45))
                            .width(1.5_f32),
                    );
                });

            let active_idx = Self::update_chart_touch_selection(
                ui,
                &plot_resp,
                recent.len(),
                &mut self.selected_sleep_idx,
            );
            if let Some(s) = recent.get(active_idx) {
                let b = s.breakdown();
                let primary = format!("{} asleep", reports::fmt_hours(b.asleep_min));
                let details = format!(
                    "Bedtime {} • Deep {:.0}m • REM {:.0}m • Light {:.0}m • Awake {:.0}m",
                    reports::fmt_clock(s.bedtime_min as f64),
                    b.deep_min,
                    b.rem_min,
                    b.light_min,
                    b.awake_min,
                );
                ui.add_space(6.0);
                p.chart_readout_bar(ui, &s.date.to_string(), &primary, &details);
            }
        });

        p.section_header(ui, "Recent Nights", None);
        for s in self.store.sleep.iter().rev().take(14) {
            let b = s.breakdown();
            p.card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(s.date.to_string()).size(15.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(reports::fmt_hours(b.asleep_min))
                                .size(16.0)
                                .strong()
                                .color(p.primary),
                        );
                    });
                });
                ui.label(
                    RichText::new(format!(
                        "Bedtime {} • Total in bed {}",
                        reports::fmt_clock(s.bedtime_min as f64),
                        reports::fmt_hours(b.in_bed_min)
                    ))
                    .size(12.5)
                    .color(p.muted),
                );
                ui.add_space(4.0);
                p.sleep_stage_bar(ui, &b);
            });
        }
    }

    // -----------------------------------------------------------------------
    // Body (Xiaomi S400) Tab
    // -----------------------------------------------------------------------

    fn ui_body(&mut self, ui: &mut Ui) {
        let p = self.pal();
        let today = Local::now().date_naive();
        let profile = self.store.profile();
        let age = profile.age_on(today);

        p.section_header(ui, "Xiaomi Scale S400", None);

        // Live BLE card + Manual Weigh-in
        p.card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let scan_resp = if self.bridge.scanning {
                    p.icon_button(ui, Icon::Stop, "Stop Scale Scan", Some(p.warn))
                } else {
                    p.icon_button(ui, Icon::Radar, "Start Scale Scan", Some(p.primary))
                };
                if scan_resp.clicked() {
                    let next = !self.bridge.scanning;
                    self.bridge.set_ble_scanning(next);
                }

                if p.icon_button(ui, Icon::Sparkle, "Test S400", None).clicked() {
                    let (mac, key) = self.bridge.simulate_s400_broadcast();
                    if self.store.settings.bindkey.trim().is_empty() {
                        self.store.settings.bindkey = key.to_string();
                        self.form_bindkey = key.to_string();
                    }
                    self.status_msg = format!(
                        "Simulated S400 weigh-in from {mac} (69.9 kg, 543.2 Ω / 497.6 Ω)."
                    );
                }
            });

            if self.store.settings.bindkey.trim().len() != 32 {
                ui.label(
                    RichText::new(
                        "Paste your 32-character BLE bindkey in Settings to decrypt live S400 broadcasts.",
                    )
                    .size(12.5)
                    .color(p.warn),
                );
            } else if let Some((mac, rssi)) = &self.last_seen_mac {
                ui.label(
                    RichText::new(format!(
                        "Last signal: {mac} ({rssi} dBm) • Decoded packets: {}",
                        self.assembler.packets_seen
                    ))
                    .size(12.5)
                    .color(p.muted),
                );
            }

            ui.collapsing("+ Log Manual Weigh-in", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Weight (kg):");
                    ui.add(egui::TextEdit::singleline(&mut self.manual_weight).desired_width(65.0));
                    ui.label("Impedance Ω:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.manual_impedance)
                            .hint_text("optional")
                            .desired_width(70.0),
                    );
                    if ui.button("Save").clicked() {
                        if let Ok(w) = self.manual_weight.trim().parse::<f32>() {
                            if (20.0..=250.0).contains(&w) {
                                let imp = self.manual_impedance.trim().parse::<f32>().ok();
                                let bf = self.manual_body_fat.trim().parse::<f32>().ok();
                                self.store.add_measurement(ScaleMeasurement {
                                    time_ms: Local::now().timestamp_millis(),
                                    weight_kg: w,
                                    impedance_low: imp,
                                    impedance_high: None,
                                    heart_rate: None,
                                    profile_id: None,
                                    reported_body_fat: bf,
                                    source: MeasurementSource::Manual,
                                });
                                self.save_store();
                                self.status_msg = format!("Saved manual weigh-in: {w:.1} kg.");
                            }
                        }
                    }
                });
            });
        });

        if let Some(m) = self.store.latest_measurement().cloned() {
            let c = bodycomp::compute(&m, &profile, age);
            p.section_header(ui, "Latest Body Composition", None);
            Self::ui_body_detail_card(p, ui, &m, &c, &profile, age);
        }

        if !self.store.scale.is_empty() {
            let scale_copy: Vec<_> = self
                .store
                .scale
                .iter()
                .rev()
                .take(21)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            p.card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Scale Trend").size(15.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .selectable_label(self.body_show_bf, "Body Fat (%)")
                            .clicked()
                        {
                            self.body_show_bf = true;
                        }
                        if ui
                            .selectable_label(!self.body_show_bf, "Weight (kg)")
                            .clicked()
                        {
                            self.body_show_bf = false;
                        }
                    });
                });

                let sel_body = self
                    .selected_body_idx
                    .map(|i| i.min(scale_copy.len() - 1))
                    .unwrap_or(scale_copy.len() - 1);

                let pts_vec: Vec<[f64; 2]> = scale_copy
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        let y = if self.body_show_bf {
                            bodycomp::compute(m, &profile, age)
                                .body_fat_pct
                                .unwrap_or(m.weight_kg) as f64
                        } else {
                            m.weight_kg as f64
                        };
                        [i as f64, y]
                    })
                    .collect();

                let sel_pt: Vec<[f64; 2]> = pts_vec
                    .iter()
                    .copied()
                    .filter(|pt| (pt[0] - sel_body as f64).abs() < 0.5)
                    .collect();

                let line_col = if self.body_show_bf {
                    p.good
                } else {
                    p.primary
                };

                let plot_resp = Plot::new("body_plot")
                    .height(165.0)
                    .allow_zoom(false)
                    .allow_drag(false)
                    .allow_scroll(false)
                    .show_x(false)
                    .show_y(false)
                    .show_grid([false, true])
                    .show_axes([false, true])
                    .label_formatter(|_, _| String::new())
                    .show(ui, |plot_ui| {
                        plot_ui.line(
                            Line::new(PlotPoints::from(pts_vec.clone()))
                                .color(line_col)
                                .width(2.5_f32),
                        );
                        plot_ui.points(
                            Points::new(PlotPoints::from(pts_vec))
                                .color(line_col)
                                .radius(4.0_f32),
                        );
                        plot_ui.vline(
                            VLine::new(sel_body as f64)
                                .color(p.warn.gamma_multiply(0.6))
                                .width(1.5_f32),
                        );
                        if !sel_pt.is_empty() {
                            plot_ui.points(
                                Points::new(PlotPoints::from(sel_pt))
                                    .color(p.warn)
                                    .radius(6.5_f32),
                            );
                        }
                    });

                let active_idx = Self::update_chart_touch_selection(
                    ui,
                    &plot_resp,
                    scale_copy.len(),
                    &mut self.selected_body_idx,
                );
                if let Some(m) = scale_copy.get(active_idx) {
                    let c = bodycomp::compute(m, &profile, age);
                    let dt = DateTime::from_timestamp_millis(m.time_ms)
                        .map(|d| {
                            d.with_timezone(&Local)
                                .format("%Y-%m-%d %H:%M")
                                .to_string()
                        })
                        .unwrap_or_else(|| "–".into());
                    let primary = format!(
                        "{:.1} kg{}",
                        m.weight_kg,
                        c.body_fat_pct
                            .map(|bf| format!(" • {bf:.1}% fat"))
                            .unwrap_or_default()
                    );
                    let details = format!(
                        "BMI {:.1} • Muscle {} • Visceral {} • Water {}",
                        c.bmi,
                        c.muscle_mass_kg
                            .map(|v| format!("{v:.1} kg"))
                            .unwrap_or_else(|| "–".into()),
                        c.visceral_fat
                            .map(|v| format!("Lv {v:.0}"))
                            .unwrap_or_else(|| "–".into()),
                        c.water_pct
                            .map(|v| format!("{v:.1}%"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    ui.add_space(6.0);
                    p.chart_readout_bar(ui, &dt, &primary, &details);
                }
            });

            p.section_header(ui, "Weigh-in History", None);
            let mut to_delete = None;
            for m in self.store.scale.iter().rev().take(20) {
                let c = bodycomp::compute(m, &profile, age);
                let dt = DateTime::from_timestamp_millis(m.time_ms)
                    .map(|d| {
                        d.with_timezone(&Local)
                            .format("%Y-%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "–".into());
                p.card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{:.1} kg", m.weight_kg))
                                .size(17.0)
                                .strong()
                                .color(p.primary),
                        );
                        if let Some(bf) = c.body_fat_pct {
                            ui.label(
                                RichText::new(format!("• {bf:.1}% fat"))
                                    .size(15.0)
                                    .strong(),
                            );
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if p.delete_icon_button(ui).clicked() {
                                to_delete = Some(m.time_ms);
                            }
                            ui.label(RichText::new(dt).size(12.0).color(p.muted));
                        });
                    });
                    ui.label(
                        RichText::new(format!(
                            "BMI {:.1} • Muscle {} • Water {} • Visceral {} • HR {}",
                            c.bmi,
                            c.muscle_mass_kg
                                .map(|v| format!("{v:.1} kg"))
                                .unwrap_or_else(|| "–".into()),
                            c.water_pct
                                .map(|v| format!("{v:.1}%"))
                                .unwrap_or_else(|| "–".into()),
                            c.visceral_fat
                                .map(|v| format!("{v:.0}"))
                                .unwrap_or_else(|| "–".into()),
                            m.heart_rate
                                .map(|v| format!("{v} bpm"))
                                .unwrap_or_else(|| "–".into()),
                        ))
                        .size(12.5)
                        .color(p.muted),
                    );
                });
            }
            if let Some(ts) = to_delete {
                self.store.remove_measurement(ts);
                self.save_store();
            }
        }
    }

    fn ui_body_detail_card(
        p: Palette,
        ui: &mut Ui,
        m: &ScaleMeasurement,
        c: &BodyComposition,
        profile: &UserProfile,
        age: u32,
    ) {
        let dt = DateTime::from_timestamp_millis(m.time_ms)
            .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "–".into());

        p.card(ui, |ui| {
            let col_w = (ui.available_width() - 16.0) * 0.5;
            egui::Grid::new("body_detail_2col")
                .num_columns(2)
                .min_col_width(col_w)
                .spacing([16.0, 12.0])
                .show(ui, |ui| {
                    Self::mini_stat(
                        ui,
                        p,
                        &format!("Weight (ideal {:.1} kg)", c.ideal_weight_kg),
                        &format!("{:.1} kg", c.weight_kg),
                    );
                    let bf_label = if let Some(bf) = c.body_fat_pct {
                        let r = bodycomp::rate_body_fat(bf, profile.sex, age);
                        format!("Body Fat ({})", r.label())
                    } else {
                        "Body Fat".to_string()
                    };
                    Self::mini_stat(
                        ui,
                        p,
                        &bf_label,
                        &c.body_fat_pct
                            .map(|v| format!("{v:.1} %"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    ui.end_row();

                    Self::mini_stat(
                        ui,
                        p,
                        &format!("BMI ({})", bodycomp::rate_bmi(c.bmi).label()),
                        &format!("{:.1}", c.bmi),
                    );
                    Self::mini_stat(
                        ui,
                        p,
                        "Muscle Mass",
                        &c.muscle_mass_kg
                            .map(|v| format!("{v:.1} kg"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    ui.end_row();

                    Self::mini_stat(
                        ui,
                        p,
                        "Body Water",
                        &c.water_pct
                            .map(|v| format!("{v:.1} %"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    Self::mini_stat(
                        ui,
                        p,
                        "Visceral Fat",
                        &c.visceral_fat
                            .map(|v| format!("Level {v:.0}"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    ui.end_row();

                    Self::mini_stat(
                        ui,
                        p,
                        "BMR (Daily Base)",
                        &c.bmr_kcal
                            .map(|v| format!("{v:.0} kcal"))
                            .unwrap_or_else(|| "–".into()),
                    );
                    Self::mini_stat(
                        ui,
                        p,
                        "Bone / Protein",
                        &format!(
                            "{} / {}",
                            c.bone_mass_kg
                                .map(|v| format!("{v:.2} kg"))
                                .unwrap_or_else(|| "–".into()),
                            c.protein_pct
                                .map(|v| format!("{v:.1}%"))
                                .unwrap_or_else(|| "–".into())
                        ),
                    );
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "Measured {dt} • Scale HR: {} • Metabolic age: {}",
                    m.heart_rate
                        .map(|h| format!("{h} bpm"))
                        .unwrap_or_else(|| "–".into()),
                    c.metabolic_age
                        .map(|a| format!("{a:.0} yrs"))
                        .unwrap_or_else(|| "–".into())
                ))
                .size(12.0)
                .color(p.muted),
            );
        });
    }

    // -----------------------------------------------------------------------
    // Reports Tab
    // -----------------------------------------------------------------------

    fn get_report(&mut self, period: Period) -> Report {
        let today = Local::now().date_naive();
        if let Some((p, d, r)) = &self.cached_report {
            if *p == period && *d == today {
                return r.clone();
            }
        }
        let r = reports::build_report(&self.store, period, today);
        self.cached_report = Some((period, today, r.clone()));
        r
    }

    fn ui_reports(&mut self, ui: &mut Ui) {
        let p = self.pal();
        p.section_header(ui, "Reports & Recommendations", None);

        let report = self.get_report(self.report_period);
        p.card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for per in [Period::Week, Period::Month, Period::Quarter] {
                    let sel = self.report_period == per;
                    let txt = RichText::new(format!("{} ({}d)", per.label(), per.days()))
                        .size(14.0)
                        .color(if sel { p.primary } else { p.text })
                        .strong();
                    if ui.selectable_label(sel, txt).clicked() {
                        self.report_period = per;
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if p.icon_button(ui, Icon::Clipboard, "Copy", None).clicked() {
                        let txt = report.to_text();
                        ui.output_mut(|o| o.copied_text = txt.clone());
                        self.bridge.copy_to_system(txt);
                        self.status_msg = "Copied report to clipboard.".into();
                    }
                });
            });
            ui.label(
                RichText::new(format!("Period: {} → {}", report.start, report.end))
                    .size(12.5)
                    .color(p.muted),
            );
        });

        let report = self.get_report(self.report_period);
        for n in &report.notes {
            ui.colored_label(p.warn, format!("Note: {n}"));
        }

        p.section_header(
            ui,
            "Recommendations",
            Some(&format!("• {}", report.recommendations.len())),
        );
        for r in &report.recommendations {
            p.recommendation_card(ui, r);
        }

        p.section_header(ui, "Summary Numbers", None);
        for sec in &report.sections {
            p.card(ui, |ui| {
                ui.label(
                    RichText::new(&sec.title)
                        .size(16.0)
                        .strong()
                        .color(p.primary),
                );
                ui.add_space(4.0);
                for st in &sec.stats {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&st.label).size(13.5).color(p.muted));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if let Some(chg) = &st.change {
                                ui.label(RichText::new(chg).size(12.0).color(p.muted));
                            }
                            ui.label(RichText::new(&st.value).size(14.0).strong());
                        });
                    });
                }
            });
        }

        ui.add_space(4.0);
        ui.label(RichText::new(reports::DISCLAIMER).size(12.0).color(p.muted));
    }

    // -----------------------------------------------------------------------
    // Settings Tab (with one-tap Android Clipboard Paste!)
    // -----------------------------------------------------------------------

    fn ui_settings(&mut self, ctx: &egui::Context, ui: &mut Ui) {
        let p = self.pal();
        p.section_header(ui, "Xiaomi S400 Scale Key", None);
        p.card(ui, |ui| {
            ui.label(
                RichText::new(
                    "Copy your 32-character S400 BLE bindkey onto your phone's clipboard, then tap 'Paste Key' below:",
                )
                .size(13.0)
                .color(p.muted),
            );
            ui.add_space(6.0);

            ui.label(RichText::new("32-character BLE Bindkey:").strong());
            ui.horizontal_wrapped(|ui| {
                if p.icon_button(ui, Icon::Clipboard, "Paste Key", Some(p.primary))
                    .clicked()
                {
                    self.bridge.paste_from_system(ClipboardTarget::Bindkey);
                }
                if p.clear_button(ui, "Clear").clicked() {
                    self.form_bindkey.clear();
                }
                let len = self.form_bindkey.trim().len();
                let status_col = if len == 32 { p.good } else { p.warn };
                ui.label(
                    RichText::new(format!("{len}/32 hex chars"))
                        .size(12.5)
                        .color(status_col),
                );
            });
            ui.add(
                egui::TextEdit::singleline(&mut self.form_bindkey)
                    .hint_text("e.g. 0728974d657a4b60964c1b1677f35f7c")
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(6.0);
            ui.label(RichText::new("Scale MAC Address (optional — auto-detected):").size(13.0));
            ui.horizontal_wrapped(|ui| {
                if p.icon_button(ui, Icon::Clipboard, "Paste MAC", None).clicked() {
                    self.bridge.paste_from_system(ClipboardTarget::ScaleMac);
                }
                if p.clear_button(ui, "Clear").clicked() {
                    self.form_scale_mac.clear();
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut self.form_scale_mac)
                    .hint_text("AA:BB:CC:DD:EE:FF")
                    .desired_width(f32::INFINITY),
            );
        });

        p.section_header(ui, "Personal Profile", Some("• For body composition"));
        p.card(ui, |ui| {
            egui::Grid::new("profile_settings_grid")
                .num_columns(2)
                .spacing([18.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Height (cm):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.form_height_cm).desired_width(110.0),
                    );
                    ui.end_row();

                    ui.label("Birth Year:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.form_birth_year).desired_width(110.0),
                    );
                    ui.end_row();

                    ui.label("Biological Sex:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.form_sex, Sex::Male, "Male");
                        ui.radio_value(&mut self.form_sex, Sex::Female, "Female");
                    });
                    ui.end_row();

                    ui.label("Scale Profile Slot (optional):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.form_profile_slot)
                            .hint_text("1–8")
                            .desired_width(110.0),
                    );
                    ui.end_row();
                });

            ui.add_space(8.0);
            if p.icon_button(
                ui,
                Icon::Check,
                "Save Profile & Scale Settings",
                Some(p.primary),
            )
            .clicked()
            {
                let height = self
                    .form_height_cm
                    .trim()
                    .parse::<f32>()
                    .unwrap_or(175.0)
                    .clamp(100.0, 240.0);
                let birth_year = self
                    .form_birth_year
                    .trim()
                    .parse::<i32>()
                    .unwrap_or(1990)
                    .clamp(1910, 2025);
                self.store.profile = Some(UserProfile {
                    height_cm: height,
                    birth_year,
                    sex: self.form_sex,
                });
                self.store.settings.bindkey = self.form_bindkey.trim().to_string();
                self.store.settings.scale_mac = if self.form_scale_mac.trim().is_empty() {
                    None
                } else {
                    Some(self.form_scale_mac.trim().to_string())
                };
                self.store.settings.scale_profile_id =
                    self.form_profile_slot.trim().parse::<u8>().ok();
                self.save_store();
                self.status_msg = "Settings saved.".into();
            }
        });

        p.section_header(ui, "App & Data", None);
        p.card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Health Connect sync range:");
                for d in [30i64, 90, 365] {
                    ui.selectable_value(&mut self.sync_days_back, d, format!("{d}d"));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Appearance:");
                let (theme_icon, theme_lbl) = if self.dark_mode {
                    (Icon::Moon, "Dark Mode (tap for Light)")
                } else {
                    (Icon::Sun, "Light Mode (tap for Dark)")
                };
                if p.icon_button(ui, theme_icon, theme_lbl, None).clicked() {
                    self.dark_mode = !self.dark_mode;
                    self.apply_theme(ctx);
                }
            });
            ui.label(
                RichText::new(format!(
                    "Health Connect: {} • Fitbit Perms: {} • Scale BLE Perms: {}",
                    if self.hc_available { "Ready" } else { "Unavailable" },
                    if self.health_granted { "Granted" } else { "Not Granted" },
                    if self.ble_granted { "Granted" } else { "Not Granted" },
                ))
                .size(12.5)
                .color(p.muted),
            );
            ui.horizontal_wrapped(|ui| {
                if p.icon_button(ui, Icon::Key, "Permissions", None).clicked() {
                    self.bridge.request_permissions();
                    self.bridge.check_permissions();
                }
                if p.icon_button(ui, Icon::Sparkle, "Demo Data", None).clicked() {
                    platform::populate_demo_data(&mut self.store);
                    self.save_store();
                    self.status_msg = "Loaded 35 days of demo data.".into();
                }
                if p.trash_button(ui, "Clear Data").clicked() {
                    self.store.days.clear();
                    self.store.sleep.clear();
                    self.store.scale.clear();
                    self.save_store();
                    self.status_msg = "Cleared stored data.".into();
                }
            });
        });
    }

    fn fmt_thousands(n: u64) -> String {
        let s = n.to_string();
        let mut out = String::new();
        for (i, ch) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                out.push(',');
            }
            out.push(ch);
        }
        out.chars().rev().collect()
    }
}

impl eframe::App for HealthHubApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events();
        if self.bridge.scanning {
            ctx.request_repaint_after(StdDuration::from_millis(500));
        }

        let p = self.pal();

        egui::TopBottomPanel::top("top_header")
            .frame(
                Frame::default()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .inner_margin(Margin {
                        left: 16.0,
                        right: 16.0,
                        top: self.safe_top,
                        bottom: 12.0,
                    }),
            )
            .show(ctx, |ui| {
                self.ui_header(ui);
            });

        egui::TopBottomPanel::bottom("bottom_nav")
            .frame(
                Frame::default()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .inner_margin(Margin {
                        left: 8.0,
                        right: 8.0,
                        top: 8.0,
                        bottom: self.safe_bottom,
                    }),
            )
            .show(ctx, |ui| {
                self.ui_bottom_nav(ui);
            });

        egui::CentralPanel::default()
            .frame(
                Frame::default()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        match self.tab {
                            Tab::Today => self.ui_today(ui),
                            Tab::Activity => self.ui_activity(ui),
                            Tab::Sleep => self.ui_sleep(ui),
                            Tab::Body => self.ui_body(ui),
                            Tab::Reports => self.ui_reports(ui),
                            Tab::Settings => self.ui_settings(ctx, ui),
                        }
                        ui.add_space(16.0);
                    });
            });
    }
}
