//! The Insights dashboard: cards, a trend chart, a weekly heatmap, people,
//! words, kept messages and saving. Charts are painted with egui's painter
//! in the app's palette.

use egui::{Align, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2, vec2};

use super::stats::{Kind, Period, Reach, Stats};
use super::{Choice, Tab};
use crate::app::App;
use crate::extras::keep::Why;
use crate::model::Action;
use crate::theme::Palette;

const GAP: f32 = 10.0;

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// The sent colour, the received colour.
fn pair(palette: &Palette) -> (Color32, Color32) {
    (palette.accent, mix(palette.accent, palette.surface, 0.55))
}

fn kind_color(kind: Kind, palette: &Palette) -> Color32 {
    let base = match kind {
        Kind::Text => palette.accent,
        Kind::Photo => Color32::from_rgb(0x4F, 0x8D, 0xF7),
        Kind::Video => Color32::from_rgb(0xA8, 0x6B, 0xF2),
        Kind::Voice => Color32::from_rgb(0xF2, 0x9C, 0x38),
        Kind::Document => Color32::from_rgb(0x2B, 0xB3, 0xA6),
        Kind::Sticker => Color32::from_rgb(0xEC, 0x5F, 0x8C),
        Kind::Other => palette.secondary,
    };
    if palette.dark {
        mix(base, Color32::WHITE, 0.1)
    } else {
        base
    }
}

fn card(ui: &mut Ui, palette: &Palette, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(14.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(
                    RichText::new(title)
                        .font(crate::theme::semibold(14.0))
                        .color(palette.text),
                );
                ui.add_space(8.0);
            }
            add(ui);
        });
}

fn day_label(day: i32, with_year: bool) -> String {
    let date = jiff::civil::date(1970, 1, 1) + jiff::Span::new().days(i64::from(day));
    if with_year {
        date.strftime("%b %-d, %Y").to_string()
    } else {
        date.strftime("%b %-d").to_string()
    }
}

fn weekday_name(day: u8) -> &'static str {
    [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ][day as usize % 7]
}

fn hour_label(hour: u8) -> String {
    match hour {
        0 => "12 am".into(),
        12 => "12 pm".into(),
        1..=11 => format!("{hour} am"),
        _ => format!("{} pm", hour - 12),
    }
}

pub fn duration(seconds: i64) -> String {
    match seconds {
        ..60 => "under a minute".into(),
        60..3600 => format!("{} min", seconds / 60),
        _ => {
            let (hours, minutes) = (seconds / 3600, seconds % 3600 / 60);
            if minutes == 0 {
                format!("{hours} h")
            } else {
                format!("{hours} h {minutes} min")
            }
        }
    }
}

fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    super::poll(app);
    if app.extras.insights.fetch.running() {
        // The run's next request is due even when nothing else redraws.
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
    if !app.extras.insights.open {
        return;
    }
    let mut open = true;
    let screen = ctx.content_rect();
    let width = 900.0_f32.min(screen.width() - 40.0).max(360.0);
    let height = (screen.height() - 80.0).clamp(360.0, 900.0);
    egui::Window::new(RichText::new("Insights").strong())
        .id(egui::Id::new("extras-insights-window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(width, height))
        .default_pos(screen.center() - vec2(width, height) / 2.0)
        .show(ctx, |ui| dashboard(app, ui));
    if !open {
        app.extras.insights.open = false;
    }
}

fn dashboard(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    header(app, ui, &palette);
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        for (tab, label) in [
            (Tab::Overview, "Overview".to_owned()),
            (Tab::People, "People".to_owned()),
            (Tab::Rhythm, "Rhythm".to_owned()),
            (Tab::Words, "Words".to_owned()),
            (
                Tab::Kept,
                match app.extras.insights.kept.len() {
                    0 => "Deleted & edited".to_owned(),
                    n => format!("Deleted & edited ({n})"),
                },
            ),
            (Tab::Saving, "Saving".to_owned()),
            (Tab::History, "History".to_owned()),
        ] {
            if ui
                .selectable_label(app.extras.insights.tab == tab, label)
                .clicked()
            {
                app.extras.insights.tab = tab;
            }
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("extras-insights-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = GAP;
            match app.extras.insights.tab {
                Tab::Kept => kept(app, ui, &palette),
                Tab::Saving => saving(app, ui, &palette),
                Tab::History => history(app, ui, &palette),
                tab => numbers(app, ui, &palette, tab),
            }
        });
}

fn header(app: &mut App, ui: &mut Ui, palette: &Palette) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            let source = app
                .extras
                .insights
                .current()
                .map_or("This account".to_owned(), |source| source.name.clone());
            ui.label(
                RichText::new(source)
                    .font(crate::theme::semibold(18.0))
                    .color(palette.text),
            );
            ui.label(
                RichText::new("Computed on this computer. Nothing leaves it.")
                    .font(crate::theme::regular(12.0))
                    .color(palette.secondary),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let insights = &app.extras.insights;
            if insights.loading() {
                ui.spinner();
            } else if ui
                .button("↻ Refresh")
                .on_hover_text("Read the archive again")
                .clicked()
            {
                super::refresh(app);
            }
            if ui
                .add_enabled(
                    !app.extras.insights.picking(),
                    egui::Button::new("Import chat export…"),
                )
                .on_hover_text(
                    "A .txt or .zip from WhatsApp's Export chat. Read in memory only, never saved.",
                )
                .clicked()
            {
                super::pick_export(app);
            }
            let insights = &mut app.extras.insights;
            if !insights.imports.is_empty() {
                let current = match insights.choice {
                    Choice::Account => "This account".to_owned(),
                    Choice::Imported(i) => insights
                        .imports
                        .get(i)
                        .map_or(String::new(), |source| source.name.clone()),
                };
                egui::ComboBox::from_id_salt("extras-insights-source")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut insights.choice, Choice::Account, "This account");
                        for (i, source) in insights.imports.iter().enumerate() {
                            ui.selectable_value(
                                &mut insights.choice,
                                Choice::Imported(i),
                                source.name.as_str(),
                            );
                        }
                    });
            }
        });
    });
    // An export waiting to be told which name is the person's, then which
    // chat it is.
    let mut chosen = None;
    let mut into: Option<Option<String>> = None;
    let mut cancel = false;
    if let Some(pending) = &app.extras.insights.pending {
        match &pending.me {
            None => card(ui, palette, "Which of these is you?", |ui| {
                ui.label(
                    RichText::new(format!(
                        "\"{}\" has {} messages. Pick your name so sent and received are told apart.",
                        pending.export.title,
                        pending.export.lines.len()
                    ))
                    .color(palette.secondary),
                );
                ui.horizontal_wrapped(|ui| {
                    for (name, count) in pending.senders.iter().take(12) {
                        if ui.button(format!("{name} · {count}")).clicked() {
                            chosen = Some(name.clone());
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            }),
            Some(_) => card(ui, palette, "Which chat is it?", |ui| {
                ui.label(
                    RichText::new(
                        "Merged into a chat, the export adds the messages this computer does not have to this account's numbers. Only for this session; nothing is written to the archive.",
                    )
                    .color(palette.secondary),
                );
                ui.horizontal_wrapped(|ui| {
                    for (index, (id, title)) in pending.chats.iter().take(8).enumerate() {
                        let button = if index == 0 {
                            egui::Button::new(RichText::new(format!("Merge into {title}")).strong())
                        } else {
                            egui::Button::new(format!("Merge into {title}"))
                        };
                        if ui.add(button).clicked() {
                            into = Some(Some(id.clone()));
                        }
                    }
                    if ui
                        .button("Keep separate")
                        .on_hover_text("Show the export as a source of its own, as before")
                        .clicked()
                    {
                        into = Some(None);
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            }),
        }
    }
    if let Some(me) = chosen {
        super::import_as(app, &me);
    }
    if let Some(chat) = into {
        super::merge_into(app, chat);
    }
    if cancel {
        app.extras.insights.pending = None;
    }
    if let Some(notice) = app.extras.insights.notice.clone() {
        ui.horizontal(|ui| {
            ui.colored_label(palette.warning, notice);
            if ui.small_button("✕").clicked() {
                app.extras.insights.notice = None;
            }
        });
    }
}

fn period_chips(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        for period in Period::ALL {
            if ui
                .selectable_label(app.extras.insights.period == period, period.label())
                .clicked()
            {
                app.extras.insights.period = period;
            }
        }
    });
}

fn numbers(app: &mut App, ui: &mut Ui, palette: &Palette, tab: Tab) {
    if tab == Tab::Words && !app.extras.config.content_analysis {
        words_consent(app, ui, palette);
        return;
    }
    if let (Some(error), Choice::Account) = (&app.extras.insights.error, app.extras.insights.choice)
    {
        card(ui, palette, "The archive could not be read", |ui| {
            ui.colored_label(palette.danger, error.as_str());
        });
        return;
    }
    period_chips(app, ui);
    let content_analysis = app.extras.config.content_analysis;
    let Some(stats) = app.extras.insights.stats().cloned() else {
        card(ui, palette, "", |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading your messages on this computer…");
            });
        });
        return;
    };
    if stats.total == 0 {
        card(ui, palette, "Nothing in this period", |ui| {
            ui.label(
                RichText::new("Choose a longer period, or import a chat export.")
                    .color(palette.secondary),
            );
        });
        return;
    }
    match tab {
        Tab::Overview => overview(app, ui, palette, &stats),
        Tab::People => people(app, ui, palette, &stats),
        Tab::Rhythm => rhythm(ui, palette, &stats),
        Tab::Words if words(ui, palette, &stats, content_analysis) => {
            app.extras.config.content_analysis = false;
            app.extras.dirty = true;
            app.extras.insights.forget_text();
        }
        _ => {}
    }
}

/// A card's small line under its number, in its colour.
type Note = (String, Color32);

fn kpi(ui: &mut Ui, palette: &Palette, value: &str, label: &str, note: Option<Note>) {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(14.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(label)
                    .font(crate::theme::regular(12.0))
                    .color(palette.secondary),
            );
            ui.label(
                RichText::new(value)
                    .font(crate::theme::bold(24.0))
                    .color(palette.text),
            );
            match note {
                Some((note, color)) => {
                    ui.label(
                        RichText::new(note)
                            .font(crate::theme::medium(11.5))
                            .color(color),
                    );
                }
                None => {
                    ui.label(RichText::new(" ").font(crate::theme::regular(11.5)));
                }
            }
        });
}

fn overview(app: &mut App, ui: &mut Ui, palette: &Palette, stats: &Stats) {
    if app.extras.insights.choice == Choice::Account {
        coverage_line(app, ui, palette);
    }
    let change = stats.previous_total.and_then(|previous| {
        if previous == 0 {
            return None;
        }
        let delta = (stats.total as f32 - previous as f32) / previous as f32 * 100.0;
        let color = if delta >= 0.0 {
            palette.accent
        } else {
            palette.warning
        };
        Some((
            format!(
                "{} {:.0}% vs the {} before",
                if delta >= 0.0 { "▲" } else { "▼" },
                delta.abs(),
                app.extras.insights.period.label()
            ),
            color,
        ))
    });
    let columns = if ui.available_width() > 640.0 { 3 } else { 2 };
    let sent_share = stats.sent as f32 / stats.total.max(1) as f32 * 100.0;
    let cards: Vec<(String, &str, Option<Note>)> = vec![
        (thousands(stats.total), "Messages", change),
        (
            thousands(stats.sent),
            "Sent by you",
            Some((format!("{sent_share:.0}% of all"), palette.secondary)),
        ),
        (thousands(stats.received), "Received", None),
        (
            thousands(stats.active_chats),
            "Active chats",
            Some((
                format!("on {} of {} days", stats.active_days, stats.days),
                palette.secondary,
            )),
        ),
        (
            format!("{:.1}", stats.daily_average()),
            "Messages a day",
            None,
        ),
        (
            format!("{} days", stats.current_streak),
            "Current streak",
            Some((
                format!("longest {} days", stats.longest_streak),
                palette.secondary,
            )),
        ),
    ];
    for row in cards.chunks(columns) {
        ui.columns(columns, |columns| {
            for (column, (value, label, note)) in columns.iter_mut().zip(row.iter().cloned()) {
                kpi(column, palette, &value, label, note);
            }
        });
    }
    card(ui, palette, "Highlights", |ui| {
        highlights(ui, palette, stats)
    });
    card(ui, palette, "Activity", |ui| trend(ui, palette, stats));
    ui.columns(2, |columns| {
        card(&mut columns[0], palette, "What gets shared", |ui| {
            kinds(ui, palette, stats)
        });
        card(&mut columns[1], palette, "Where you talk", |ui| {
            split(
                ui,
                palette,
                &[
                    ("Private chats", stats.private_messages, palette.accent),
                    (
                        "Groups",
                        stats.group_messages,
                        Color32::from_rgb(0x4F, 0x8D, 0xF7),
                    ),
                ],
            );
            ui.add_space(8.0);
            let started = stats.started_by_me + stats.started_by_them;
            if started > 0 {
                split(
                    ui,
                    palette,
                    &[
                        ("You start", stats.started_by_me, palette.accent),
                        (
                            "They start",
                            stats.started_by_them,
                            mix(palette.accent, palette.surface, 0.55),
                        ),
                    ],
                );
            }
        });
    });
}

fn highlights(ui: &mut Ui, palette: &Palette, stats: &Stats) {
    let mut lines: Vec<String> = Vec::new();
    if let Some((day, count)) = stats.busiest_day {
        lines.push(format!(
            "Your busiest day was {}, with {count} messages.",
            day_label(day, true)
        ));
    }
    if let Some((hour, _)) = stats.busiest_hour {
        lines.push(format!("You are most active around {}.", hour_label(hour)));
    }
    if let Some((weekday, _)) = stats.busiest_weekday {
        lines.push(format!(
            "{}s are your busiest day of the week.",
            weekday_name(weekday)
        ));
    }
    if let Some(top) = stats.top.first() {
        lines.push(format!(
            "{} is your most active chat: {:.0}% of your messages.",
            top.title,
            top.share * 100.0
        ));
    }
    let started = stats.started_by_me + stats.started_by_them;
    if started >= 5 {
        lines.push(format!(
            "You start {:.0}% of your private conversations.",
            stats.started_by_me as f32 / started as f32 * 100.0
        ));
    }
    match (stats.my_reply, stats.their_reply) {
        (Some(mine), Some(theirs)) => lines.push(format!(
            "You usually reply in {}; others reply to you in {}.",
            duration(mine),
            duration(theirs)
        )),
        (Some(mine), None) => lines.push(format!("You usually reply in {}.", duration(mine))),
        _ => {}
    }
    if !stats.quiet.is_empty() {
        let names: Vec<&str> = stats
            .quiet
            .iter()
            .take(3)
            .map(|q| q.title.as_str())
            .collect();
        lines.push(format!(
            "You haven't talked with {} in a month. See People.",
            names.join(", ")
        ));
    }
    for line in lines {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("•").color(palette.accent));
            ui.label(RichText::new(line).color(palette.text));
        });
    }
}

fn legend(ui: &mut Ui, palette: &Palette, items: &[(&str, Color32)]) {
    ui.horizontal_wrapped(|ui| {
        for (label, color) in items {
            let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().rect_filled(rect, 3.0, *color);
            ui.label(
                RichText::new(*label)
                    .font(crate::theme::regular(12.0))
                    .color(palette.secondary),
            );
            ui.add_space(6.0);
        }
    });
}

fn trend(ui: &mut Ui, palette: &Palette, stats: &Stats) {
    let (sent_color, received_color) = pair(palette);
    legend(
        ui,
        palette,
        &[("Sent", sent_color), ("Received", received_color)],
    );
    let height = 150.0;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height + 18.0), Sense::hover());
    let chart = Rect::from_min_size(rect.min, vec2(rect.width(), height));
    let painter = ui.painter_at(rect);
    let max = stats
        .series
        .iter()
        .map(|bucket| bucket.sent + bucket.received)
        .max()
        .unwrap_or(0)
        .max(1);
    // Guides at a half and the top.
    for fraction in [0.5, 1.0] {
        let y = chart.bottom() - chart.height() * fraction;
        painter.line_segment(
            [pos2(chart.left(), y), pos2(chart.right(), y)],
            Stroke::new(1.0, mix(palette.outline, palette.surface, 0.3)),
        );
    }
    painter.text(
        pos2(chart.left(), chart.top()),
        egui::Align2::LEFT_TOP,
        thousands(max),
        FontId::proportional(10.5),
        palette.secondary,
    );
    let count = stats.series.len().max(1) as f32;
    let slot = chart.width() / count;
    let bar = (slot * 0.7).clamp(1.0, 22.0);
    let mut hovered = None;
    for (i, bucket) in stats.series.iter().enumerate() {
        let x = chart.left() + slot * (i as f32 + 0.5);
        let received_h = chart.height() * bucket.received as f32 / max as f32;
        let sent_h = chart.height() * bucket.sent as f32 / max as f32;
        let radius = (bar / 3.0).min(4.0);
        if received_h > 0.0 {
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(x - bar / 2.0, chart.bottom() - received_h - sent_h),
                    pos2(x + bar / 2.0, chart.bottom() - sent_h),
                ),
                radius,
                received_color,
            );
        }
        if sent_h > 0.0 {
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(x - bar / 2.0, chart.bottom() - sent_h),
                    pos2(x + bar / 2.0, chart.bottom()),
                ),
                radius,
                sent_color,
            );
        }
        if let Some(pointer) = response.hover_pos()
            && (pointer.x - x).abs() <= slot / 2.0
        {
            hovered = Some(bucket.clone());
            painter.line_segment(
                [pos2(x, chart.top()), pos2(x, chart.bottom())],
                Stroke::new(1.0, palette.outline),
            );
        }
    }
    let with_year = stats.days > 300;
    if let (Some(first), Some(last)) = (stats.series.first(), stats.series.last()) {
        painter.text(
            pos2(chart.left(), rect.bottom()),
            egui::Align2::LEFT_BOTTOM,
            day_label(first.day, with_year),
            FontId::proportional(11.0),
            palette.secondary,
        );
        painter.text(
            pos2(chart.right(), rect.bottom()),
            egui::Align2::RIGHT_BOTTOM,
            day_label(last.day, with_year),
            FontId::proportional(11.0),
            palette.secondary,
        );
    }
    if let Some(bucket) = hovered {
        let when = match bucket.span {
            1 => day_label(bucket.day, true),
            7 => format!("Week of {}", day_label(bucket.day, true)),
            _ => format!("From {}", day_label(bucket.day, true)),
        };
        response.on_hover_text(format!(
            "{when}\n{} sent · {} received",
            bucket.sent, bucket.received
        ));
    }
}

fn split(ui: &mut Ui, palette: &Palette, parts: &[(&str, u32, Color32)]) {
    let total: u32 = parts.iter().map(|(_, n, _)| n).sum::<u32>().max(1);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 7.0, mix(palette.outline, palette.surface, 0.5));
    let mut x = rect.left();
    for (_, n, color) in parts {
        let w = rect.width() * *n as f32 / total as f32;
        if w > 0.5 {
            painter.rect_filled(
                Rect::from_min_max(pos2(x, rect.top()), pos2(x + w, rect.bottom())),
                7.0,
                *color,
            );
        }
        x += w;
    }
    ui.horizontal_wrapped(|ui| {
        for (label, n, color) in parts {
            let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().rect_filled(dot, 3.0, *color);
            ui.label(
                RichText::new(format!("{label} {:.0}%", *n as f32 / total as f32 * 100.0))
                    .font(crate::theme::medium(12.0))
                    .color(palette.text),
            );
            ui.add_space(8.0);
        }
    });
}

fn kinds(ui: &mut Ui, palette: &Palette, stats: &Stats) {
    let total: u32 = stats.kinds.iter().map(|(_, n)| n).sum::<u32>().max(1);
    let max = stats
        .kinds
        .iter()
        .map(|(_, n)| *n)
        .max()
        .unwrap_or(1)
        .max(1);
    for (kind, n) in &stats.kinds {
        let (row, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let painter = ui.painter_at(row);
        let label_width = 104.0;
        let count_width = 64.0;
        painter.text(
            pos2(row.left(), row.center().y),
            egui::Align2::LEFT_CENTER,
            kind.label(),
            crate::theme::regular(12.5),
            palette.text,
        );
        let track = Rect::from_min_max(
            pos2(row.left() + label_width, row.center().y - 5.0),
            pos2(row.right() - count_width, row.center().y + 5.0),
        );
        painter.rect_filled(track, 5.0, mix(palette.outline, palette.surface, 0.5));
        let filled = Rect::from_min_size(
            track.min,
            vec2(
                (track.width() * *n as f32 / max as f32).max(4.0),
                track.height(),
            ),
        );
        painter.rect_filled(filled, 5.0, kind_color(*kind, palette));
        painter.text(
            pos2(row.right(), row.center().y),
            egui::Align2::RIGHT_CENTER,
            format!(
                "{} · {:.0}%",
                thousands(*n),
                *n as f32 / total as f32 * 100.0
            ),
            crate::theme::medium(11.5),
            palette.secondary,
        );
        response.on_hover_text(format!("{}: {}", kind.label(), thousands(*n)));
    }
}

fn people(app: &mut App, ui: &mut Ui, palette: &Palette, stats: &Stats) {
    let (sent_color, received_color) = pair(palette);
    let mut open = None;
    card(ui, palette, "Top chats", |ui| {
        legend(
            ui,
            palette,
            &[("You", sent_color), ("Them", received_color)],
        );
        let max = stats.top.first().map_or(1, |chat| chat.total).max(1);
        for chat in &stats.top {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let initial: String = chat.title.chars().take(1).collect();
                let (circle, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
                ui.painter().circle_filled(
                    circle.center(),
                    15.0,
                    mix(palette.accent, palette.surface, 0.75),
                );
                ui.painter().text(
                    circle.center(),
                    egui::Align2::CENTER_CENTER,
                    initial.to_uppercase(),
                    crate::theme::semibold(13.0),
                    palette.accent,
                );
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&chat.title)
                                .font(crate::theme::semibold(13.5))
                                .color(palette.text),
                        );
                        if chat.group {
                            ui.label(RichText::new("group").small().color(palette.secondary));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !chat.id.starts_with("export:") && ui.small_button("Open").clicked()
                            {
                                open = Some(chat.id.clone());
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{} · {:.0}%",
                                    thousands(chat.total),
                                    chat.share * 100.0
                                ))
                                .color(palette.secondary),
                            );
                        });
                    });
                    let width = ui.available_width() * chat.total as f32 / max as f32;
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(width.max(4.0), 8.0), Sense::hover());
                    let mine = rect.width() * chat.sent as f32 / chat.total.max(1) as f32;
                    ui.painter().rect_filled(rect, 4.0, received_color);
                    ui.painter().rect_filled(
                        Rect::from_min_size(rect.min, vec2(mine, rect.height())),
                        4.0,
                        sent_color,
                    );
                    let mut replies = Vec::new();
                    if let Some(mine) = chat.my_reply {
                        replies.push(format!("you reply in {}", duration(mine)));
                    }
                    if let Some(theirs) = chat.their_reply {
                        replies.push(format!("they reply in {}", duration(theirs)));
                    }
                    if !replies.is_empty() {
                        ui.label(
                            RichText::new(replies.join(" · "))
                                .font(crate::theme::regular(11.5))
                                .color(palette.secondary),
                        );
                    }
                });
            });
        }
    });
    if !stats.quiet.is_empty() {
        card(ui, palette, "Gone quiet", |ui| {
            ui.label(
                RichText::new(
                    "People you talked with often, but not in the last 30 days. Maybe say hi?",
                )
                .color(palette.secondary),
            );
            for quiet in &stats.quiet {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&quiet.title).color(palette.text).strong());
                    let days = (jiff::Timestamp::now().as_second() - quiet.last_at) / 86_400;
                    ui.label(
                        RichText::new(format!(
                            "{} messages before · last {days} days ago",
                            quiet.before
                        ))
                        .color(palette.secondary),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !quiet.id.starts_with("export:") && ui.small_button("Open").clicked() {
                            open = Some(quiet.id.clone());
                        }
                    });
                });
            }
        });
    }
    if let Some(id) = open {
        app.actions.push(Action::OpenChat(id));
    }
}

fn rhythm(ui: &mut Ui, palette: &Palette, stats: &Stats) {
    card(ui, palette, "Your week", |ui| {
        let max = stats
            .heatmap
            .iter()
            .flat_map(|row| row.iter())
            .copied()
            .max()
            .unwrap_or(0)
            .max(1);
        let label_width = 34.0;
        let cell = ((ui.available_width() - label_width) / 24.0).clamp(8.0, 30.0);
        let (rect, response) = ui.allocate_exact_size(
            vec2(label_width + cell * 24.0, cell * 7.0 + 18.0),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        let mut tip = None;
        for (weekday, row) in stats.heatmap.iter().enumerate() {
            let y = rect.top() + weekday as f32 * cell;
            painter.text(
                pos2(rect.left(), y + cell / 2.0),
                egui::Align2::LEFT_CENTER,
                &weekday_name(weekday as u8)[..3],
                FontId::proportional(11.0),
                palette.secondary,
            );
            for (hour, count) in row.iter().enumerate() {
                let x = rect.left() + label_width + hour as f32 * cell;
                let square = Rect::from_min_size(pos2(x, y), Vec2::splat(cell)).shrink(1.5);
                let t = (*count as f32 / max as f32).sqrt();
                let color = if *count == 0 {
                    mix(palette.outline, palette.surface, 0.55)
                } else {
                    mix(palette.surface, palette.accent, 0.18 + 0.82 * t)
                };
                painter.rect_filled(square, 3.0, color);
                if response.hover_pos().is_some_and(|p| square.contains(p)) {
                    painter.rect_stroke(
                        square,
                        3.0,
                        Stroke::new(1.5, palette.text),
                        egui::StrokeKind::Outside,
                    );
                    tip = Some(format!(
                        "{}, {}: {} messages",
                        weekday_name(weekday as u8),
                        hour_label(hour as u8),
                        count
                    ));
                }
            }
        }
        for hour in (0..24).step_by(3) {
            painter.text(
                pos2(
                    rect.left() + label_width + hour as f32 * cell,
                    rect.bottom(),
                ),
                egui::Align2::LEFT_BOTTOM,
                hour_label(hour as u8),
                FontId::proportional(10.5),
                palette.secondary,
            );
        }
        if let Some(tip) = tip {
            response.on_hover_text(tip);
        }
    });
    ui.columns(2, |columns| {
        card(&mut columns[0], palette, "By hour", |ui| {
            let hours: Vec<u32> = (0..24)
                .map(|hour| stats.heatmap.iter().map(|row| row[hour]).sum())
                .collect();
            bars(ui, palette, &hours, |i| hour_label(i as u8), 6);
        });
        card(&mut columns[1], palette, "By weekday", |ui| {
            let days: Vec<u32> = stats.heatmap.iter().map(|row| row.iter().sum()).collect();
            bars(
                ui,
                palette,
                &days,
                |i| weekday_name(i as u8)[..3].to_owned(),
                1,
            );
        });
    });
    card(ui, palette, "", |ui| {
        if let Some((hour, count)) = stats.busiest_hour {
            ui.label(format!(
                "Peak hour: {} ({} messages)",
                hour_label(hour),
                thousands(count)
            ));
        }
        if let Some((day, count)) = stats.busiest_weekday {
            ui.label(format!(
                "Busiest weekday: {} ({} messages)",
                weekday_name(day),
                thousands(count)
            ));
        }
        ui.label(format!(
            "Longest streak of days you wrote: {} · current: {}",
            stats.longest_streak, stats.current_streak
        ));
    });
}

fn bars(
    ui: &mut Ui,
    palette: &Palette,
    values: &[u32],
    label: impl Fn(usize) -> String,
    every: usize,
) {
    let max = values.iter().copied().max().unwrap_or(0).max(1);
    let height = 90.0;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height + 16.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let slot = rect.width() / values.len().max(1) as f32;
    let mut tip = None;
    for (i, value) in values.iter().enumerate() {
        let x = rect.left() + slot * i as f32;
        let h = height * *value as f32 / max as f32;
        let bar = Rect::from_min_max(
            pos2(x + slot * 0.15, rect.top() + height - h),
            pos2(x + slot * 0.85, rect.top() + height),
        );
        let hovered = response
            .hover_pos()
            .is_some_and(|p| p.x >= x && p.x < x + slot);
        painter.rect_filled(
            bar,
            3.0,
            if hovered {
                palette.accent_hover
            } else {
                palette.accent
            },
        );
        if hovered {
            tip = Some(format!("{}: {}", label(i), value));
        }
        if i % every == 0 {
            let (anchor, align) = if i == 0 {
                (pos2(x, rect.bottom()), egui::Align2::LEFT_BOTTOM)
            } else {
                (
                    pos2(x + slot / 2.0, rect.bottom()),
                    egui::Align2::CENTER_BOTTOM,
                )
            };
            painter.text(
                anchor,
                align,
                label(i),
                FontId::proportional(10.0),
                palette.secondary,
            );
        }
    }
    if let Some(tip) = tip {
        response.on_hover_text(tip);
    }
}

fn words_consent(app: &mut App, ui: &mut Ui, palette: &Palette) {
    card(ui, palette, "Read what was said? (optional)", |ui| {
        ui.label(
            RichText::new(
                "Everything else in Insights comes from when and with whom you talk, never \
                 from what was said. Turn this on to also see your most used words and emoji, \
                 and how long your messages are. The text is read on this computer, in memory \
                 only, and nothing derived from it is saved or sent anywhere.",
            )
            .color(palette.secondary),
        );
        ui.add_space(6.0);
        if ui.button("Analyse message text on this computer").clicked() {
            app.extras.config.content_analysis = true;
            app.extras.dirty = true;
            super::refresh(app);
        }
    });
}

/// Returns whether the person turned content analysis off.
fn words(ui: &mut Ui, palette: &Palette, stats: &Stats, on: bool) -> bool {
    let Some(words) = stats.words.as_ref().filter(|_| on) else {
        card(ui, palette, "", |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading text for word statistics… Press Refresh if this stays.");
            });
        });
        return false;
    };
    if words.texts == 0 {
        card(ui, palette, "No text messages in this period", |_| {});
        return false;
    }
    card(ui, palette, "Most used words", |ui| {
        let max = words.top_words.first().map_or(1, |(_, n)| *n).max(1);
        ui.horizontal_wrapped(|ui| {
            for (word, count) in &words.top_words {
                let t = *count as f32 / max as f32;
                let size = 12.0 + 12.0 * t.sqrt();
                let color = mix(palette.secondary, palette.accent, t.sqrt());
                ui.label(
                    RichText::new(word)
                        .font(crate::theme::semibold(size))
                        .color(color),
                )
                .on_hover_text(format!("{count} times"));
                ui.add_space(4.0);
            }
        });
    });
    ui.columns(2, |columns| {
        card(&mut columns[0], palette, "Favourite emoji", |ui| {
            if words.top_emoji.is_empty() {
                ui.label(RichText::new("No emoji yet.").color(palette.secondary));
            }
            ui.horizontal_wrapped(|ui| {
                for (emoji, count) in &words.top_emoji {
                    ui.label(RichText::new(emoji).size(24.0))
                        .on_hover_text(format!("{count} times"));
                    ui.label(
                        RichText::new(count.to_string())
                            .small()
                            .color(palette.secondary),
                    );
                    ui.add_space(4.0);
                }
            });
        });
        card(&mut columns[1], palette, "Message length", |ui| {
            ui.label(format!(
                "You: {:.0} characters on average",
                words.my_average
            ));
            ui.label(format!(
                "Others: {:.0} characters on average",
                words.their_average
            ));
            ui.label(
                RichText::new(format!("From {} text messages", thousands(words.texts)))
                    .color(palette.secondary),
            );
        });
    });
    let mut stop = false;
    card(ui, palette, "", |ui| {
        ui.label(
            RichText::new("Text is analysed in memory and dropped when you turn this off.")
                .color(palette.secondary),
        );
        stop = ui.button("Stop reading message text").clicked();
    });
    stop
}

fn kept(app: &mut App, ui: &mut Ui, palette: &Palette) {
    card(ui, palette, "", |ui| {
        ui.label(
            RichText::new(
                "Messages their sender deleted for everyone, and the versions edits replaced, \
                 stay here in your local archive. Nothing about this reaches WhatsApp or the \
                 sender. Messages you delete yourself are not kept.",
            )
            .color(palette.secondary),
        );
    });
    if app.extras.insights.kept.is_empty() {
        card(ui, palette, "Nothing kept yet", |ui| {
            ui.label(
                RichText::new("When someone deletes a message for everyone, it shows up here.")
                    .color(palette.secondary),
            );
        });
        return;
    }
    let archive = super::archive_path(app);
    let mut forget = None;
    let mut open = None;
    for (index, item) in app.extras.insights.kept.iter().enumerate().take(500) {
        let title = app.chat_title_of(&item.chat);
        let sender = if item.from_me {
            "You".to_owned()
        } else {
            item.sender_name
                .clone()
                .unwrap_or_else(|| app.chat_title_of(&item.sender))
        };
        egui::Frame::new()
            .fill(palette.surface)
            .stroke(Stroke::new(1.0, palette.outline))
            .corner_radius(12.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (badge, color) = match item.why {
                        Why::Deleted => ("Deleted for everyone".to_owned(), palette.danger),
                        Why::Edited => (format!("Before edit {}", item.version), palette.warning),
                    };
                    ui.label(
                        RichText::new(badge)
                            .font(crate::theme::semibold(11.5))
                            .color(color),
                    );
                    let who = if sender == title {
                        title.clone()
                    } else {
                        format!("{title} · {sender}")
                    };
                    ui.label(
                        RichText::new(who)
                            .font(crate::theme::semibold(13.0))
                            .color(palette.text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button("Forget")
                            .on_hover_text("Remove this copy")
                            .clicked()
                        {
                            forget = Some(index);
                        }
                        if ui.small_button("Open chat").clicked() {
                            open = Some(item.chat.clone());
                        }
                        let when = jiff::Timestamp::from_second(item.timestamp)
                            .map(|t| {
                                t.to_zoned(jiff::tz::TimeZone::system())
                                    .strftime("%b %-d, %Y %H:%M")
                                    .to_string()
                            })
                            .unwrap_or_default();
                        ui.label(RichText::new(when).small().color(palette.secondary));
                    });
                });
                let summary = item.content.summary();
                ui.label(RichText::new(summary).color(palette.text));
                if let Some(path) = item.content.media().and_then(|media| media.path.as_ref()) {
                    ui.label(
                        RichText::new(format!("Saved at {}", path.display()))
                            .small()
                            .color(palette.secondary),
                    );
                }
            });
    }
    if let Some(index) = forget {
        let item = app.extras.insights.kept.remove(index);
        crate::extras::keep::request_forget(&archive, &item);
    }
    if let Some(chat) = open {
        app.actions.push(Action::OpenChat(chat));
    }
}

fn saving(app: &mut App, ui: &mut Ui, palette: &Palette) {
    card(ui, palette, "Keep everything on this computer", |ui| {
        let mut save = app.extras.config.save_all_media;
        if ui
            .checkbox(&mut save, "Save every attachment as it arrives")
            .on_hover_text(
                "Photos, videos, voice notes, documents and stickers, whatever their size",
            )
            .changed()
        {
            app.extras.config.save_all_media = save;
            app.extras.dirty = true;
        }
        ui.label(
            RichText::new(
                "Messages, deleted messages and edit history are always kept in your local \
                 archive on this computer, encrypted with a key in your system keychain when \
                 one is available.",
            )
            .color(palette.secondary),
        );
    });
    let missing = app.extras.insights.missing.len();
    let (running, asked, total) = (
        app.extras.backfill.running(),
        app.extras.backfill.asked,
        app.extras.backfill.total,
    );
    let (mut stop, mut start, mut count) = (false, false, false);
    card(ui, palette, "Attachments not on disk yet", |ui| {
        if running {
            let done = asked as f32 / total.max(1) as f32;
            ui.add(egui::ProgressBar::new(done).text(format!("Asked for {asked} of {total}")));
            stop = ui.button("Stop").clicked();
        } else {
            ui.label(format!(
                "{} attachments in your archive are not downloaded.",
                thousands(missing as u32)
            ));
            ui.label(
                RichText::new(
                    "WhatsApp keeps attachments on its servers for a limited time: older ones \
                     may no longer download.",
                )
                .color(palette.secondary),
            );
            ui.horizontal(|ui| {
                start = ui
                    .add_enabled(missing > 0, egui::Button::new("Save everything now"))
                    .clicked();
                count = ui.button("Count again").clicked();
            });
        }
    });
    if stop {
        app.extras.backfill.stop();
    }
    if start {
        let missing = std::mem::take(&mut app.extras.insights.missing);
        app.extras.backfill.start(missing);
    }
    if count {
        super::refresh(app);
    }
}

/// From when the numbers run, and how many chats are complete
/// (REQ-EXTRAS-011.A2).
fn coverage_line(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let list = app.extras.insights.coverage();
    let Some(oldest) = list.iter().map(|chat| chat.oldest_at).min() else {
        return;
    };
    let full = list
        .iter()
        .filter(|chat| chat.reach != Reach::Partial)
        .count();
    let total = list.len();
    let since = super::source::local(oldest, &jiff::tz::TimeZone::system()).0;
    let mut go = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!(
                "Numbers run from {} · {full} of {total} chats have their full history",
                day_label(since, true)
            ))
            .font(crate::theme::regular(12.5))
            .color(palette.secondary),
        );
        if full < total && ui.link("Fill in older history").clicked() {
            go = true;
        }
    });
    if go {
        app.extras.insights.tab = Tab::History;
    }
}

/// What the numbers cover, older history from the phone, and merged
/// exports (REQ-EXTRAS-009/010/011).
fn history(app: &mut App, ui: &mut Ui, palette: &Palette) {
    if app.extras.insights.loading() && app.extras.insights.coverage().is_empty() {
        card(ui, palette, "", |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading your messages on this computer…");
            });
        });
        return;
    }
    let list = app.extras.insights.coverage().to_vec();
    let partial = list
        .iter()
        .filter(|chat| chat.reach == Reach::Partial)
        .count();
    let connected = matches!(app.account().link, crate::backend::LinkStatus::Connected);
    card(ui, palette, "What the numbers cover", |ui| {
        ui.label(
            RichText::new(
                "Insights counts the messages on this computer. Your phone sent recent history when you linked it; older messages stay on the phone until they are asked for. Fill the gaps by fetching older history from your phone, or by merging a chat export.",
            )
            .color(palette.secondary),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "{} chats · {} with full history · {} with an export · {partial} partial",
                list.len(),
                list.iter().filter(|chat| chat.reach == Reach::Full).count(),
                list.iter()
                    .filter(|chat| chat.reach == Reach::WithExport)
                    .count(),
            ))
            .color(palette.text),
        );
    });

    // Older history from the phone.
    let mut start = false;
    let mut stop = false;
    let mut pick_top = false;
    let mut clear = false;
    card(ui, palette, "Fetch older history from your phone", |ui| {
        let insights = &app.extras.insights;
        ui.label(
            RichText::new(format!(
                "Asks your phone for older messages in the chats ticked below, the way scrolling to the top of a chat does: up to {} messages per chat, one request every {} seconds. Only your own phone is asked. On an iPhone, WhatsApp may show \"Syncing\" while it answers.",
                super::fetch::PAGES * 50,
                super::fetch::GAP.as_secs()
            ))
            .color(palette.secondary),
        );
        ui.add_space(4.0);
        if insights.fetch.running() {
            let (asked, cap) = insights.fetch.progress();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("Asked {asked} of at most {cap} times"));
                if !connected {
                    ui.colored_label(palette.warning, "Waiting for your phone to connect");
                }
                if ui.button("Stop").clicked() {
                    stop = true;
                }
            });
            ui.add(egui::ProgressBar::new(asked as f32 / cap.max(1) as f32).desired_height(6.0));
        } else {
            ui.horizontal_wrapped(|ui| {
                let chosen = insights.selected.len();
                let label = match chosen {
                    0 => "Fetch older history".to_owned(),
                    1 => "Fetch older history for 1 chat".to_owned(),
                    n => format!("Fetch older history for {n} chats"),
                };
                let button = ui
                    .add_enabled(chosen > 0 && connected, egui::Button::new(label))
                    .on_disabled_hover_text(if connected {
                        "Tick the chats to fetch below"
                    } else {
                        "Your phone is not connected"
                    });
                if button.clicked() {
                    start = true;
                }
                if partial > 0 && ui.button("Tick the 10 busiest partial chats").clicked() {
                    pick_top = true;
                }
                if chosen > 0 && ui.button("Clear").clicked() {
                    clear = true;
                }
            });
        }
    });
    if pick_top {
        let insights = &mut app.extras.insights;
        insights.selected.extend(
            list.iter()
                .filter(|chat| chat.reach == Reach::Partial)
                .take(10)
                .map(|chat| chat.id.clone()),
        );
    }
    if clear {
        app.extras.insights.selected.clear();
    }
    if start {
        super::start_fetch(app);
    }
    if stop {
        app.extras.insights.fetch.stop();
    }

    // Exports merged in this session.
    let mut remove = None;
    if !app.extras.insights.merges.is_empty() {
        card(ui, palette, "Merged exports", |ui| {
            for (index, merge) in app.extras.insights.merges.iter().enumerate() {
                ui.horizontal(|ui| {
                    let title = app.chat_title_of(&merge.chat);
                    ui.label(RichText::new(title).strong().color(palette.text));
                    ui.label(
                        RichText::new(format!(
                            "from \"{}\" · {} messages read",
                            merge.export,
                            thousands(merge.records.len() as u32)
                        ))
                        .color(palette.secondary),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                });
            }
        });
    }
    if let Some(index) = remove {
        super::unmerge(app, index);
    }

    // Every chat, most messages first.
    let mut toggle = None;
    let mut open = None;
    let running = app.extras.insights.fetch.running();
    card(ui, palette, "Chats", |ui| {
        if list.is_empty() {
            ui.label(RichText::new("No messages on this computer yet.").color(palette.secondary));
        }
        let zone = jiff::tz::TimeZone::system();
        for chat in list.iter().take(300) {
            ui.horizontal(|ui| {
                let can_fetch = chat.reach != Reach::Full;
                let ticked = app.extras.insights.selected.contains(&chat.id);
                let enabled = can_fetch && !running;
                // An outlined box: egui's own checkbox has no frame in this
                // theme when it is not ticked.
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                let response = response.on_hover_text(match (can_fetch, running) {
                    (false, _) => "Your phone said there is nothing older",
                    (true, true) => "A fetch is running",
                    (true, false) => "Fetch this chat's older history",
                });
                if can_fetch {
                    let stroke = if enabled && response.hovered() {
                        palette.text
                    } else {
                        palette.secondary
                    };
                    let fill = if ticked {
                        palette.accent
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect(
                        rect.shrink(1.0),
                        4.0,
                        fill,
                        Stroke::new(1.5, if ticked { palette.accent } else { stroke }),
                        egui::StrokeKind::Inside,
                    );
                    if ticked {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "✓",
                            crate::theme::bold(12.0),
                            palette.surface,
                        );
                    }
                }
                if enabled && response.clicked() {
                    toggle = Some((chat.id.clone(), !ticked));
                }
                let title = if app.extras.insights.fetch.is_target(&chat.id) {
                    format!("{} ⟳", chat.title)
                } else {
                    chat.title.clone()
                };
                ui.label(RichText::new(title).strong().color(palette.text));
                if chat.group {
                    ui.label(RichText::new("group").small().color(palette.secondary));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Open").clicked() {
                        open = Some(chat.id.clone());
                    }
                    let color = match chat.reach {
                        Reach::Full => palette.accent,
                        Reach::WithExport => Color32::from_rgb(0x4F, 0x8D, 0xF7),
                        Reach::Partial => palette.secondary,
                    };
                    ui.label(RichText::new(chat.reach.label()).color(color));
                    let since = super::source::local(chat.oldest_at, &zone).0;
                    let merged = if chat.merged > 0 {
                        format!(" ({} from export)", thousands(chat.merged))
                    } else {
                        String::new()
                    };
                    ui.label(
                        RichText::new(format!(
                            "since {} · {} messages{merged}",
                            day_label(since, true),
                            thousands(chat.messages)
                        ))
                        .color(palette.secondary),
                    );
                });
            });
        }
    });
    if let Some((id, on)) = toggle {
        let selected = &mut app.extras.insights.selected;
        if on {
            selected.insert(id);
        } else {
            selected.remove(&id);
        }
    }
    if let Some(id) = open {
        app.actions.push(Action::OpenChat(id));
    }
}
