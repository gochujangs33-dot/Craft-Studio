//! Dark, hairline-bordered visual language matching the official ArtCraft launcher.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x0B, 0x0B, 0x0C);
pub const PANEL: Color32 = Color32::from_rgb(0x11, 0x11, 0x13);
pub const CARD: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x18, 0x18, 0x1C);
pub const LINE: Color32 = Color32::from_rgb(0x25, 0x25, 0x2A);
pub const INK: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xEE);
pub const MUTED: Color32 = Color32::from_rgb(0x8C, 0x8C, 0x94);
pub const DIM: Color32 = Color32::from_rgb(0x5C, 0x5C, 0x64);
pub const OK: Color32 = Color32::from_rgb(0x3E, 0xD5, 0x98);
pub const ERR: Color32 = Color32::from_rgb(0xF2, 0x55, 0x5A);

pub fn hex(s: &str) -> Color32 {
    let s = s.trim_start_matches('#');
    match u32::from_str_radix(s, 16) {
        Ok(v) if s.len() == 6 => Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8),
        _ => INK,
    }
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0_f32, 1.0_f32);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()), f(a.a(), b.a()))
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn apply(ctx: &egui::Context) {
    // ------------------------------------------------ fonts (Korean support)
    let mut fonts = FontDefinitions::default();
    let malgun_paths = [
        "C:\\Windows\\Fonts\\malgun.ttf",
        "C:\\Windows\\Fonts\\malgunbd.ttf",
        "C:\\Windows\\Fonts\\gulim.ttc",
    ];
    for p in malgun_paths {
        if let Ok(data) = std::fs::read(p) {
            fonts.font_data.insert("korean_font".to_owned(), Arc::new(FontData::from_owned(data)));
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "korean_font".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("korean_font".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);

    // ------------------------------------------------ visuals
    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x0E, 0x0E, 0x10);
    v.faint_bg_color = Color32::from_rgb(0x10, 0x10, 0x12);
    v.window_stroke = Stroke::new(1.0_f32, LINE);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.selection.bg_fill = Color32::from_rgb(0x2B, 0x2B, 0x33);
    v.selection.stroke = Stroke::new(1.0_f32, INK);
    v.hyperlink_color = INK;

    let w = &mut v.widgets;
    for wv in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        wv.corner_radius = CornerRadius::ZERO;
        wv.expansion = 0.0_f32;
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    w.inactive.bg_fill = Color32::from_rgb(0x15, 0x15, 0x18);
    w.inactive.weak_bg_fill = w.inactive.bg_fill;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
    w.hovered.bg_fill = Color32::from_rgb(0x20, 0x20, 0x25);
    w.hovered.weak_bg_fill = w.hovered.bg_fill;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x48, 0x48, 0x50));
    w.hovered.fg_stroke = Stroke::new(1.0_f32, INK);
    w.active.bg_fill = Color32::from_rgb(0x2A, 0x2A, 0x30);
    w.active.weak_bg_fill = w.active.bg_fill;
    w.active.bg_stroke = Stroke::new(1.0_f32, INK);
    w.active.fg_stroke = Stroke::new(1.0_f32, INK);
    w.open = w.hovered;
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.interaction.tooltip_delay = 0.0_f32;
        s.spacing.item_spacing = egui::vec2(8.0_f32, 6.0_f32);
        s.spacing.button_padding = egui::vec2(10.0_f32, 5.0_f32);
        s.spacing.interact_size.y = 24.0_f32;
        s.text_styles.insert(TextStyle::Body, FontId::proportional(13.0_f32));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(13.0_f32));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.0_f32));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0_f32));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(11.0_f32));
    });
}
