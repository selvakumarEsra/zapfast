//! The Znga panel (Today, Drafts, Ask, Setup) and its button on the rail.
//! Views only edit Znga's own fields and queue work through [`super::hooks`].

use egui::{Color32, Pos2, Rect, RichText, Sense, Ui, Vec2, vec2};

use crate::app::App;
use crate::model::Action;

use super::config::{Engine, chat_key};
use super::drafts::Kind;
use super::privacy::{ChatFacts, visible};
use super::prompt::{self, Rewrite};
use super::{Purpose, Tab, hooks, keys};

/// Paints Znga's mark: a four-point sparkle with a small one beside it.
pub fn paint_sparkle(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let star = |c: Pos2, s: f32| {
        let w = s * 0.3;
        painter.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(0.0, -s),
                c + vec2(w, 0.0),
                c + vec2(0.0, s),
                c + vec2(-w, 0.0),
            ],
            color,
            egui::Stroke::NONE,
        ));
        painter.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-s, 0.0),
                c + vec2(0.0, -w),
                c + vec2(s, 0.0),
                c + vec2(0.0, w),
            ],
            color,
            egui::Stroke::NONE,
        ));
    };
    let main = size * 0.36;
    star(center + vec2(-size * 0.08, size * 0.08), main);
    star(center + vec2(size * 0.3, -size * 0.3), size * 0.14);
}

/// The rail's row height and button size (`ui::rail`'s private constants).
const RAIL_CELL: f32 = 48.0;
const RAIL_PLATE: f32 = 36.0;

/// Where Znga's button sits: on the rail under Calls, or above Starred while
/// the chat list is hidden (its avatars fill the space under Calls). Read
/// from where the rail drew those buttons last frame, so the rail itself
/// stays untouched.
fn launcher_rect(ctx: &egui::Context, list_hidden: bool) -> Option<Rect> {
    use crate::ui::rail::{Destination, destination_id};
    let anchor = if list_hidden {
        ctx.read_response(destination_id(Destination::Starred))?
            .rect
            .center()
            - vec2(0.0, RAIL_CELL)
    } else {
        ctx.read_response(destination_id(Destination::Calls))?
            .rect
            .center()
            + vec2(0.0, RAIL_CELL)
    };
    Some(Rect::from_center_size(anchor, Vec2::splat(RAIL_PLATE)))
}

/// Znga's button on the rail, drawn in a layer of its own: a widget inside
/// upstream's rail would shift its widget ids (and so the accessibility
/// order), and a button over the conversation would cover messages.
fn launcher(app: &mut App, ctx: &egui::Context) {
    let Some(rect) = launcher_rect(ctx, !app.sidebar_visible) else {
        return;
    };
    let palette = app.palette;
    egui::Area::new(egui::Id::new("znga-launcher"))
        .fixed_pos(rect.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(rect.size(), Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            let lit = app.znga.open;
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, lit, "Znga")
            });
            if ui.is_rect_visible(rect) {
                let hovered = response.hovered() || response.has_focus();
                if lit {
                    crate::ui::widgets::highlight_card(ui, &palette, rect, palette.surface_active);
                } else if hovered {
                    crate::ui::widgets::highlight_card(ui, &palette, rect, palette.surface_hover);
                }
                let tint = if app.znga.config.enabled {
                    palette.accent
                } else if lit || hovered {
                    palette.text
                } else {
                    palette.secondary
                };
                paint_sparkle(ui.painter(), rect.center(), 24.0, tint);
                if app.znga.unseen > 0 {
                    crate::ui::widgets::unread_indicator(
                        ui,
                        &palette,
                        rect.right_top() + vec2(-3.0, 5.0),
                        app.znga.unseen,
                        false,
                        false,
                    );
                }
            }
            let hint = if app.znga.config.enabled {
                "Znga: your AI companion"
            } else {
                "Znga: set up your AI companion"
            };
            if response.on_hover_text(hint).clicked() {
                toggle(app);
            }
        });
}

pub fn toggle(app: &mut App) {
    app.znga.open = !app.znga.open;
    if app.znga.open {
        app.znga.unseen = 0;
        if !app.znga.config.enabled {
            app.znga.tab = Tab::Setup;
        }
    }
}

fn time_label(at: i64) -> String {
    jiff::Timestamp::from_second(at)
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%a %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn card(ui: &mut Ui, palette: &crate::theme::Palette, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(egui::Stroke::new(1.0, palette.outline))
        .corner_radius(10.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(6.0);
}

/// Draws the panel when open. Called once a frame after the app's views.
pub fn show(app: &mut App, ctx: &egui::Context) {
    if app.app_lock.is_locked() || !app.is_linked() {
        return;
    }
    launcher(app, ctx);
    if !app.znga.open {
        return;
    }
    let mut open = true;
    let screen = ctx.content_rect();
    let width = 400.0_f32.min(screen.width() - 32.0).max(280.0);
    egui::Window::new(RichText::new("✨ Znga").strong())
        .id(egui::Id::new("znga-panel"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(width)
        .default_height((screen.height() - 120.0).clamp(320.0, 760.0))
        .default_pos(screen.right_top() + vec2(-width - 24.0, 56.0))
        .show(ctx, |ui| panel(app, ui));
    if !open {
        app.znga.open = false;
    }
}

fn panel(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    ui.horizontal(|ui| {
        let drafts = app.znga.drafts.len();
        for (tab, label) in [
            (Tab::Today, "Today".to_owned()),
            (
                Tab::Drafts,
                if drafts > 0 {
                    format!("Drafts ({drafts})")
                } else {
                    "Drafts".to_owned()
                },
            ),
            (Tab::Ask, "Ask".to_owned()),
            (Tab::Setup, "Setup".to_owned()),
        ] {
            if ui.selectable_label(app.znga.tab == tab, label).clicked() {
                app.znga.tab = tab;
            }
        }
        if app.znga.runner.busy() > 0 {
            ui.add(egui::Spinner::new().size(14.0));
        }
    });
    if let Some(status) = app.znga.status.clone() {
        ui.horizontal_wrapped(|ui| {
            let color = if status.error {
                palette.danger
            } else {
                palette.secondary
            };
            ui.label(RichText::new(&status.text).color(color).small());
            if ui.small_button("×").clicked() {
                app.znga.status = None;
            }
        });
    }
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("znga-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| match app.znga.tab {
            Tab::Today => today(app, ui),
            Tab::Drafts => drafts(app, ui),
            Tab::Ask => ask(app, ui),
            Tab::Setup => setup(app, ui),
        });
}

fn welcome(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    card(ui, &palette, |ui| {
        ui.label(RichText::new("Meet Znga").heading());
        ui.label("Your AI companion, inside ZapFast:");
        for line in [
            "🔔  Tells you when a message really needs you",
            "📋  Briefs you on what happened while you were away, with a to-do list",
            "🪞  Drafts replies in your own voice, and nudges you to check in with your people",
            "📣  \"Tell Mom and Alex I'm running late\": one draft each, ready to send",
            "✍️  Rewrites and translates what you are typing",
        ] {
            ui.label(line);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Runs on a model on this computer by default. Nothing is sent to anyone \
                 without your click.",
            )
            .small()
            .color(palette.secondary),
        );
        if ui.button("Set up Znga").clicked() {
            app.znga.tab = Tab::Setup;
        }
    });
}

fn today(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    if !app.znga.config.enabled {
        welcome(app, ui);
        return;
    }
    ui.horizontal(|ui| {
        let briefing = |p: &Purpose| matches!(p, Purpose::Briefing { .. });
        let running = app.znga.waiting_for(briefing);
        if ui
            .add_enabled(!running, egui::Button::new("✨ Brief me"))
            .on_hover_text("What happened, and what needs you")
            .clicked()
        {
            hooks::request_briefing(app, false);
        }
        ui.label(
            RichText::new(format!(
                "{} new since last briefing",
                app.znga.activity.pending()
            ))
            .small()
            .color(palette.secondary),
        );
    });
    ui.add_space(6.0);
    if app.znga.alerts.is_empty() && app.znga.briefings.is_empty() {
        ui.label(
            RichText::new(
                "All quiet. Znga watches messages as they arrive and speaks up when \
                 something needs you.",
            )
            .color(palette.secondary),
        );
    }
    if !app.znga.alerts.is_empty() {
        ui.label(RichText::new("Needs you").strong());
        let mut dismiss = None;
        let mut actions = Vec::new();
        for (index, alert) in app.znga.alerts.iter().enumerate().rev().take(12) {
            card(ui, &palette, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&alert.chat_title).strong());
                    ui.label(
                        RichText::new(time_label(alert.at))
                            .small()
                            .color(palette.dim),
                    );
                });
                if alert.sender_name == alert.chat_title {
                    ui.label(&alert.text);
                } else {
                    ui.label(format!("{}: {}", alert.sender_name, alert.text));
                }
                ui.label(RichText::new(&alert.reason).small().color(palette.warning));
                ui.horizontal(|ui| {
                    if ui.small_button("Open chat").clicked() {
                        actions.push((alert.account.clone(), alert.chat.clone(), false));
                    }
                    if ui.small_button("Draft reply").clicked() {
                        actions.push((alert.account.clone(), alert.chat.clone(), true));
                    }
                    if ui.small_button("Done").clicked() {
                        dismiss = Some(index);
                    }
                });
            });
        }
        if let Some(index) = dismiss {
            app.znga.alerts.remove(index);
        }
        for (account, chat, draft) in actions {
            if draft {
                hooks::request_draft(app, &account, &chat, Kind::Suggestion);
                app.znga.set_status("Znga is drafting a reply…", false);
            } else if app.account().id.as_str() == account {
                app.actions.push(Action::OpenChat(chat));
            } else {
                app.toast("Switch to that account to open this chat.");
            }
        }
    }
    if !app.znga.briefings.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new("Briefings").strong());
        for briefing in app.znga.briefings.iter().rev().take(6) {
            card(ui, &palette, |ui| {
                ui.horizontal(|ui| {
                    let kind = if briefing.proactive {
                        "While you were away"
                    } else {
                        "Briefing"
                    };
                    ui.label(RichText::new(kind).strong());
                    ui.label(
                        RichText::new(time_label(briefing.at))
                            .small()
                            .color(palette.dim),
                    );
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(briefing.text.clone());
                    }
                });
                ui.add(egui::Label::new(&briefing.text).selectable(true));
            });
        }
    }
}

fn drafts(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    if !app.znga.config.enabled {
        welcome(app, ui);
        return;
    }
    // Composer tools for the open chat.
    let open = app.open_chat.clone();
    let title = open
        .as_deref()
        .and_then(|id| app.chat(id))
        .map(|chat| app.chat_title(chat));
    card(ui, &palette, |ui| {
        match &title {
            Some(title) => ui.label(RichText::new(format!("In your chat with {title}")).strong()),
            None => ui.label(RichText::new("Open a chat to use these").color(palette.secondary)),
        };
        let has_chat = open.is_some();
        let has_text = !app.composer.trim().is_empty();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(has_chat, egui::Button::new("🪞 Suggest reply"))
                .on_disabled_hover_text("Open a chat first")
                .clicked()
                && let Some(chat) = open.clone()
            {
                let account = app.account().id.as_str().to_owned();
                hooks::request_draft(app, &account, &chat, Kind::Suggestion);
                app.znga.set_status("Znga is drafting a reply…", false);
            }
            for how in Rewrite::ALL {
                if ui
                    .add_enabled(has_chat && has_text, egui::Button::new(how.label()))
                    .on_disabled_hover_text("Type a message in the chat first")
                    .clicked()
                {
                    let text = app.composer.clone();
                    hooks::request_rewrite(app, prompt::rewrite(&text, how));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Translate to");
            ui.add(egui::TextEdit::singleline(&mut app.znga.translate_to).desired_width(110.0));
            if ui
                .add_enabled(has_chat && has_text, egui::Button::new("Translate"))
                .clicked()
            {
                let text = app.composer.clone();
                let language = app.znga.translate_to.clone();
                hooks::request_rewrite(app, prompt::translate(&text, &language));
            }
        });
    });
    // "Update my people".
    card(ui, &palette, |ui| {
        ui.label(RichText::new("📣 Update your people").strong());
        ui.add(
            egui::TextEdit::multiline(&mut app.znga.update_input)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .hint_text("Tell Mom and Alex I'll be 20 minutes late"),
        );
        let ready = !app.znga.update_input.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("Draft messages"))
            .clicked()
        {
            let instruction = std::mem::take(&mut app.znga.update_input);
            hooks::request_fan_out(app, &instruction);
        }
    });
    if app.znga.drafts.is_empty() {
        ui.label(
            RichText::new(
                "No drafts yet. Mark close ones as Twin chats in Setup and Znga prepares \
                 replies as messages arrive.",
            )
            .color(palette.secondary),
        );
        return;
    }
    let updates = app.znga.drafts.count(Kind::Update);
    if updates > 1 && ui.button(format!("Send all {updates} updates")).clicked() {
        for draft in app.znga.drafts.take_kind(Kind::Update) {
            hooks::send_draft(app, draft);
        }
        app.toast("Sending your updates.");
        return;
    }
    enum Decision {
        Send,
        Edit,
        Discard,
    }
    let mut decisions: Vec<(u64, Decision)> = Vec::new();
    let ids: Vec<u64> = app.znga.drafts.items().iter().rev().map(|d| d.id).collect();
    for id in ids {
        let Some(draft) = app.znga.drafts.get_mut(id) else {
            continue;
        };
        card(ui, &palette, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(draft.kind.label())
                        .small()
                        .color(palette.accent),
                );
                ui.label(RichText::new(format!("to {}", draft.chat_title)).strong());
                ui.label(
                    RichText::new(time_label(draft.created))
                        .small()
                        .color(palette.dim),
                );
            });
            ui.add(
                egui::TextEdit::multiline(&mut draft.text)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                let label = if draft.kind == Kind::Rewrite {
                    "Use"
                } else {
                    "Send"
                };
                if draft.kind == Kind::Rewrite {
                    if ui.button(label).clicked() {
                        decisions.push((id, Decision::Edit));
                    }
                } else {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(label).color(palette.on_accent))
                                .fill(palette.accent),
                        )
                        .clicked()
                    {
                        decisions.push((id, Decision::Send));
                    }
                    if ui.button("Edit in chat").clicked() {
                        decisions.push((id, Decision::Edit));
                    }
                }
                if ui.button("Discard").clicked() {
                    decisions.push((id, Decision::Discard));
                }
            });
        });
    }
    for (id, decision) in decisions {
        let Some(draft) = app.znga.drafts.take(id) else {
            continue;
        };
        match decision {
            Decision::Send => {
                let title = draft.chat_title.clone();
                hooks::send_draft(app, draft);
                app.toast(format!("Sent to {title}."));
            }
            Decision::Edit => hooks::use_in_composer(app, draft),
            Decision::Discard => {}
        }
    }
}

fn ask(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    if !app.znga.config.enabled {
        welcome(app, ui);
        return;
    }
    if app.znga.ask.is_empty() {
        ui.label(
            RichText::new(
                "Ask about your recent chats: \"What did Ada want?\", \"Do I owe anyone \
                 a reply?\", \"Plan my evening around what people asked\".",
            )
            .color(palette.secondary),
        );
    }
    for turn in &app.znga.ask {
        let (who, color) = if turn.from_user {
            ("You", palette.text)
        } else {
            ("Znga", palette.accent)
        };
        ui.label(RichText::new(who).small().strong().color(color));
        ui.add(egui::Label::new(&turn.text).selectable(true));
        ui.add_space(6.0);
    }
    if app.znga.waiting_for(|p| matches!(p, Purpose::Ask)) {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(12.0));
            ui.label(
                RichText::new("Znga is thinking…")
                    .small()
                    .color(palette.secondary),
            );
        });
    }
    ui.separator();
    let response = ui.add(
        egui::TextEdit::multiline(&mut app.znga.ask_input)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text("Ask Znga… (Enter to send, Shift+Enter for a new line)"),
    );
    let enter = response.has_focus()
        && ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
    ui.horizontal(|ui| {
        let send = ui.button("Ask").clicked();
        if ui.small_button("Clear thread").clicked() {
            app.znga.ask.clear();
        }
        if send || enter {
            let question = std::mem::take(&mut app.znga.ask_input);
            hooks::request_ask(app, question);
        }
    });
}

fn setup(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    ui.spacing_mut().slider_width = 150.0;
    let mut changed = false;
    let mut engine_changed = false;
    card(ui, &palette, |ui| {
        changed |= ui
            .checkbox(
                &mut app.znga.config.enabled,
                RichText::new("Turn on Znga").strong(),
            )
            .changed();
        ui.label(
            RichText::new(
                "Znga reads messages as they arrive in chats you have not excluded. It \
                 drafts; you decide what is sent.",
            )
            .small()
            .color(palette.secondary),
        );
    });

    card(ui, &palette, |ui| {
        ui.label(RichText::new("Engine").strong());
        let config = &mut app.znga.config;
        egui::ComboBox::from_id_salt("znga-engine")
            .selected_text(config.engine.label())
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                for engine in Engine::ALL {
                    if ui
                        .selectable_value(&mut config.engine, engine, engine.label())
                        .changed()
                    {
                        engine_changed = true;
                    }
                }
            });
        if engine_changed {
            config.model.clear();
            config.endpoint.clear();
            config.cloud_consent = false;
        }
        let engine = config.engine;
        egui::Grid::new("znga-engine-grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Model");
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut config.model)
                            .hint_text(engine.default_model()),
                    )
                    .changed();
                ui.end_row();
                ui.label(if engine == Engine::ClaudeCode {
                    "Program"
                } else {
                    "Address"
                });
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut config.endpoint)
                            .hint_text(engine.default_endpoint()),
                    )
                    .changed();
                ui.end_row();
            });
        match engine {
            Engine::Local => {
                ui.label(
                    RichText::new(
                        "Runs on this computer: install Ollama (ollama.com), then run \
                         `ollama pull llama3.2`. Nothing leaves your machine.",
                    )
                    .small()
                    .color(palette.secondary),
                );
            }
            Engine::ClaudeCode => {
                let found = super::provider::find_claude(&config.endpoint);
                ui.label(
                    RichText::new(match &found {
                        Some(path) => format!("Found Claude Code at {}", path.display()),
                        None => "Claude Code was not found. Install it, or enter the full path \
                                 to `claude` above."
                            .to_owned(),
                    })
                    .small()
                    .color(if found.is_some() {
                        palette.secondary
                    } else {
                        palette.warning
                    }),
                );
                ui.label(
                    RichText::new(
                        "Uses your installed Claude Code and its own sign-in (your saved \
                         login, or CLAUDE_CODE_OAUTH_TOKEN in its environment). ZapFast \
                         never reads or stores the token. Meant for your own use on this \
                         computer; requests count toward your Claude plan.",
                    )
                    .small()
                    .color(palette.secondary),
                );
            }
            _ => {}
        }
    });

    let engine = app.znga.config.engine;
    if engine.needs_key() {
        card(ui, &palette, |ui| {
            ui.label(RichText::new("API key").strong());
            match keys::source(engine) {
                Some(source) => ui.label(RichText::new(format!("Using a key {source}.")).small()),
                None => ui.label(RichText::new("No key yet.").small().color(palette.warning)),
            };
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.znga.key_input)
                        .password(true)
                        .hint_text("Paste your API key")
                        .desired_width(200.0),
                );
                if ui.button("Save").clicked() {
                    let key = std::mem::take(&mut app.znga.key_input);
                    match keys::store(engine, &key) {
                        Ok(()) => app
                            .znga
                            .set_status("Key saved in the system keychain.", false),
                        Err(error) => app.znga.set_status(error, true),
                    }
                    app.znga.refresh_key();
                }
                if ui.small_button("Forget").clicked() {
                    keys::forget(engine);
                    app.znga.refresh_key();
                }
            });
            if engine == Engine::Claude {
                ui.label(
                    RichText::new(
                        "Use an API key from console.anthropic.com. To use your Claude \
                         subscription instead, pick \"Claude Code on this computer\".",
                    )
                    .small()
                    .color(palette.secondary),
                );
            }
        });
    }

    if app.znga.config.is_cloud() {
        card(ui, &palette, |ui| {
            ui.label(RichText::new("Privacy").strong().color(palette.warning));
            ui.label(
                "With this engine, the text of recent messages from chats you have not \
                 excluded is sent to the provider to write briefings and drafts. Locked \
                 chats are never sent.",
            );
            changed |= ui
                .checkbox(
                    &mut app.znga.config.cloud_consent,
                    "I understand: let this engine read my messages",
                )
                .changed();
        });
    }

    ui.horizontal(|ui| {
        if ui.button("Test connection").clicked() {
            let prompt = super::provider::Prompt {
                system: "Reply with the single word: ready".into(),
                turns: vec![super::provider::Turn::user("Are you there?")],
                max_tokens: 1000,
                effort: super::provider::Effort::Low,
            };
            match app.znga.submit(prompt, Purpose::Test) {
                Ok(()) => app.znga.set_status("Testing…", false),
                Err(error) => app.znga.set_status(error, true),
            }
        }
    });
    ui.add_space(6.0);

    card(ui, &palette, |ui| {
        ui.label(RichText::new("Proactive").strong());
        let config = &mut app.znga.config;
        changed |= ui
            .checkbox(&mut config.alerts, "Alert me when a message needs me")
            .changed();
        changed |= ui
            .checkbox(&mut config.briefings, "Brief me after I've been away")
            .changed();
        ui.add_enabled_ui(config.briefings, |ui| {
            changed |= ui
                .add(
                    egui::Slider::new(&mut config.briefing_min_messages, 1..=50)
                        .text("messages at least"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut config.briefing_interval_minutes, 5..=240)
                        .text("minutes apart"),
                )
                .changed();
        });
        changed |= ui
            .checkbox(
                &mut config.self_notes,
                "Also send briefings to my own chat (to read on my phone)",
            )
            .changed();
        if config.self_notes {
            ui.label(
                RichText::new("At most six a day, only to your own \"Message yourself\" chat.")
                    .small()
                    .color(palette.secondary),
            );
        }
        changed |= ui
            .checkbox(&mut config.include_groups, "Include group chats")
            .changed();
    });

    card(ui, &palette, |ui| {
        ui.label(RichText::new("Your twin").strong());
        let config = &mut app.znga.config;
        egui::Grid::new("znga-twin-grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Your name");
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut config.your_name)
                            .hint_text("From your profile"),
                    )
                    .changed();
                ui.end_row();
            });
        ui.label("How you write, in your words (optional)");
        changed |= ui
            .add(
                egui::TextEdit::multiline(&mut config.voice_notes)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text("I keep it short, say \"haha\" a lot, and call my mom \"ma\""),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut config.checkin_days, 2..=30).text("days before a check-in"))
            .changed();
        let account = app.account().id.as_str().to_owned();
        let learned = app.znga.style_for(&account);
        ui.label(
            RichText::new(if learned.messages >= 3 {
                format!(
                    "Learned from {} of your messages: {}",
                    learned.messages,
                    learned.describe()
                )
            } else {
                "Znga learns your style from messages you send in chats you open.".to_owned()
            })
            .small()
            .color(palette.secondary),
        );
    });

    changed |= chat_choices(app, ui);

    if changed || engine_changed {
        app.znga.mark_dirty();
    }
    if engine_changed {
        app.znga.refresh_key();
    }
}

/// Twin and Exclude switches for the active account's recent chats.
fn chat_choices(app: &mut App, ui: &mut Ui) -> bool {
    let palette = app.palette;
    let account = app.account().id.as_str().to_owned();
    let filter = crate::util::search_key(app.znga.chat_filter.trim());
    let mut chats: Vec<(String, String, i64, bool)> = app
        .account()
        .chats
        .iter()
        .filter(|chat| !chat.locked && chat.kind != crate::model::ChatKind::Broadcast)
        .map(|chat| {
            let facts = ChatFacts::of(&account, chat);
            (
                chat.id.clone(),
                app.chat_title(chat),
                chat.last_activity,
                visible(&app.znga.config, facts) || app.znga.config.is_excluded(&account, &chat.id),
            )
        })
        .filter(|(_, title, _, shown)| {
            *shown && (filter.is_empty() || crate::util::search_key(title).contains(&filter))
        })
        .collect();
    chats.sort_by_key(|(id, _, last, _)| (!app.znga.config.is_twin(&account, id), -last));
    chats.truncate(40);
    let mut changed = false;
    card(ui, &palette, |ui| {
        ui.label(RichText::new("Your people").strong());
        ui.label(
            RichText::new(
                "Twin: Znga drafts replies as messages arrive and suggests check-ins. \
                 Exclude: Znga never reads this chat.",
            )
            .small()
            .color(palette.secondary),
        );
        ui.add(egui::TextEdit::singleline(&mut app.znga.chat_filter).hint_text("Search chats"));
        egui::Grid::new("znga-chats")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                for (id, title, _, _) in &chats {
                    let key = chat_key(&account, id);
                    let config = &mut app.znga.config;
                    let mut twin = config.twin_chats.contains(&key);
                    let mut excluded = config.excluded_chats.contains(&key);
                    ui.label(title);
                    if ui.checkbox(&mut twin, "Twin").changed() {
                        changed = true;
                        if twin {
                            config.twin_chats.insert(key.clone());
                            config.excluded_chats.remove(&key);
                        } else {
                            config.twin_chats.remove(&key);
                        }
                    }
                    if ui.checkbox(&mut excluded, "Exclude").changed() {
                        changed = true;
                        if excluded {
                            config.excluded_chats.insert(key.clone());
                            config.twin_chats.remove(&key);
                        } else {
                            config.excluded_chats.remove(&key);
                        }
                    }
                    ui.end_row();
                }
            });
    });
    if changed {
        // Forget what an excluded chat said.
        let excluded: Vec<String> = app.znga.config.excluded_chats.iter().cloned().collect();
        for key in excluded {
            if let Some((account, chat)) = key.split_once('/') {
                app.znga.activity.forget_chat(account, chat);
                app.znga.drafts.clear_chat(account, chat);
            }
        }
    }
    changed
}
