//! The extras' rail buttons, each in a layer of its own: a widget inside
//! upstream's rail would shift its widget ids (and so the accessibility
//! order). They stack under Znga's button, below Calls.

use egui::{Color32, Rect, Sense, Vec2, vec2};

use crate::app::App;
use crate::ui::rail::{Destination, destination_id};

const RAIL_CELL: f32 = 48.0;
const RAIL_PLATE: f32 = 36.0;
const ICON: f32 = 20.0;
/// Rail cells Znga's button takes before ours.
const BEFORE: f32 = if cfg!(feature = "znga") { 1.0 } else { 0.0 };

pub enum Glyph {
    Icon(crate::theme::Icon),
    Chart,
}

/// Where the `n`th extras button goes, if the rail has room for it.
fn slot(ctx: &egui::Context, list_hidden: bool, n: usize) -> Option<Rect> {
    let calls = ctx.read_response(destination_id(Destination::Calls))?.rect;
    let starred = ctx
        .read_response(destination_id(Destination::Starred))?
        .rect;
    let step = RAIL_CELL * (1.0 + BEFORE + n as f32);
    let center = if list_hidden {
        starred.center() - vec2(0.0, step)
    } else {
        calls.center() + vec2(0.0, step)
    };
    let rect = Rect::from_center_size(center, Vec2::splat(RAIL_PLATE));
    // Never over the rail's own buttons: in a short window they win.
    let clear = if list_hidden {
        rect.top() > calls.bottom() + RAIL_CELL
    } else {
        rect.bottom() + 4.0 < starred.top()
    };
    clear.then_some(rect)
}

fn paint_chart(painter: &egui::Painter, rect: Rect, color: Color32) {
    let base = rect.center().y + 7.0;
    for (i, height) in [7.0, 12.0, 9.0, 15.0].into_iter().enumerate() {
        let x = rect.center().x - 7.5 + i as f32 * 5.0;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x - 1.6, base - height),
                egui::pos2(x + 1.6, base),
            ),
            1.2,
            color,
        );
    }
}

/// One rail button: lit as upstream's destinations are, with a badge and a
/// tooltip. Returns whether it was clicked.
#[allow(clippy::too_many_arguments)]
pub fn rail_button(
    app: &App,
    ctx: &egui::Context,
    id: &str,
    rect: Rect,
    glyph: Glyph,
    lit: bool,
    badge: u32,
    label: &str,
) -> bool {
    let palette = app.palette;
    let mut clicked = false;
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(rect.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(rect.size(), Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, lit, label)
            });
            if ui.is_rect_visible(rect) {
                let hovered = response.hovered() || response.has_focus();
                if lit {
                    crate::ui::widgets::highlight_card(ui, &palette, rect, palette.surface_active);
                } else if hovered {
                    crate::ui::widgets::highlight_card(ui, &palette, rect, palette.surface_hover);
                }
                let tint = if lit || hovered {
                    palette.text
                } else {
                    palette.secondary
                };
                match glyph {
                    Glyph::Icon(icon) => crate::theme::paint_icon(ui, icon, rect, ICON, tint),
                    Glyph::Chart => paint_chart(ui.painter(), rect, tint),
                }
                if badge > 0 {
                    crate::ui::widgets::unread_indicator(
                        ui,
                        &palette,
                        rect.right_top() + vec2(-3.0, 5.0),
                        badge,
                        false,
                        false,
                    );
                }
            }
            clicked = response.on_hover_text(label).clicked();
        });
    clicked
}

/// Draws the extras each frame, after the app's own views.
pub fn show(app: &mut App, ctx: &egui::Context) {
    if app.app_lock.is_locked() || !app.is_linked() {
        return;
    }
    let hidden = !app.sidebar_visible;
    if let Some(rect) = slot(ctx, hidden, 0) {
        super::favourites::button(app, ctx, rect);
    }
    if let Some(rect) = slot(ctx, hidden, 1) {
        let lit = app.extras.insights.open;
        let badge = 0;
        if rail_button(
            app,
            ctx,
            "extras-insights-button",
            rect,
            Glyph::Chart,
            lit,
            badge,
            "Insights",
        ) {
            super::insights::toggle(app);
        }
    }
    super::insights::ui::show(app, ctx);
}
