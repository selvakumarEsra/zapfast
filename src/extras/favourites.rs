//! Favourites on the rail (REQ-EXTRAS-002): a destination like Archived that
//! shows the chats marked as favourites, here or on the phone. The list is
//! upstream's Favorites filter; this adds the way to it.

use crate::app::App;
use crate::model::{Action, ChatFilter, Page};

/// Whether the window shows the favourites now: the heart is lit.
pub fn lit(app: &App) -> bool {
    app.page == Page::Chats
        && !app.show_starred
        && !app.show_archived
        && !app.locked_folder_open()
        && app.label_filter.is_none()
        && app.chat_filter == ChatFilter::Favorites
}

/// The actions that show the favourites, from wherever the window is.
/// Like Archived, it is a destination, not a toggle.
pub fn actions(app: &App) -> Vec<Action> {
    let mut actions = Vec::new();
    if lit(app) && app.sidebar_visible {
        return actions;
    }
    if app.page != Page::Chats {
        actions.push(Action::Open(Page::Chats));
    }
    if app.show_starred {
        actions.push(Action::ToggleStarred);
    }
    // The favourites open in the list column, as the archive does.
    if !app.sidebar_visible {
        actions.push(Action::ToggleSidebar);
    }
    // Also leaves the archive, the locked folder and a label.
    actions.push(Action::SetChatFilter(ChatFilter::Favorites));
    actions
}

/// What the Chats destination adds so it leads back to every chat.
pub fn leave(app: &App, actions: &mut Vec<Action>) {
    if lit(app) {
        actions.push(Action::SetChatFilter(ChatFilter::All));
    }
}

/// Favourite chats with unread messages, for the badge.
pub fn unread(app: &App) -> u32 {
    app.chats
        .iter()
        .filter(|chat| chat.favorite && !chat.archived && !chat.locked && chat.unread > 0)
        .count() as u32
}

/// The heart on the rail.
pub fn button(app: &mut App, ctx: &egui::Context, rect: egui::Rect) {
    let lit = lit(app);
    let badge = unread(app);
    let label = crate::i18n::gettext(app.locale, "Favorites").into_owned();
    let clicked = super::ui::rail_button(
        app,
        ctx,
        "extras-favourites-button",
        rect,
        super::ui::Glyph::Icon(crate::theme::Icon::Heart),
        lit,
        badge,
        &label,
    );
    if clicked {
        let actions = actions(app);
        app.actions.extend(actions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rail::Destination;

    fn app() -> (tempfile::TempDir, App) {
        let root = tempfile::tempdir().unwrap();
        let app = App::headless(
            crate::paths::AppDirs::under(root.path()),
            crate::settings::Settings::default(),
        )
        .0;
        (root, app)
    }

    #[test]
    fn favourites_opens_like_archived_and_chats_leaves_it() {
        let (_root, mut app) = app();
        let mut ada = crate::model::Chat::new("1@s.whatsapp.net".into(), "Ada".into());
        ada.favorite = true;
        ada.last_activity = 2;
        ada.unread = 3;
        let mut bob = crate::model::Chat::new("2@s.whatsapp.net".into(), "Bob".into());
        bob.last_activity = 1;
        app.chats = vec![ada, bob];
        assert_eq!(unread(&app), 1);

        // From Calls with the list hidden: back to the list, favourites only.
        app.page = Page::Calls;
        app.sidebar_visible = false;
        assert!(matches!(
            actions(&app).as_slice(),
            [
                Action::Open(Page::Chats),
                Action::ToggleSidebar,
                Action::SetChatFilter(ChatFilter::Favorites)
            ]
        ));
        // From Starred, too.
        app.page = Page::Chats;
        app.sidebar_visible = true;
        app.show_starred = true;
        assert!(matches!(
            actions(&app).as_slice(),
            [
                Action::ToggleStarred,
                Action::SetChatFilter(ChatFilter::Favorites)
            ]
        ));
        // From the archive: SetChatFilter leaves it.
        app.show_starred = false;
        app.show_archived = true;
        assert!(matches!(
            actions(&app).as_slice(),
            [Action::SetChatFilter(ChatFilter::Favorites)]
        ));

        // Shown: the list holds the favourites, and it is not a toggle.
        app.show_archived = false;
        app.chat_filter = ChatFilter::Favorites;
        assert!(lit(&app));
        let names: Vec<_> = app.visible_chats().iter().map(|c| c.name.clone()).collect();
        assert_eq!(names, ["Ada"]);
        assert!(actions(&app).is_empty());

        // Chats leads back to every chat.
        assert!(matches!(
            Destination::Chats.actions(&app).as_slice(),
            [Action::SetChatFilter(ChatFilter::All)]
        ));
        app.chat_filter = ChatFilter::All;
        assert!(Destination::Chats.actions(&app).is_empty());
    }

    #[test]
    fn only_one_destination_is_lit() {
        let (_root, mut app) = app();
        assert!(Destination::Chats.lit(&app));
        app.chat_filter = ChatFilter::Favorites;
        assert!(lit(&app));
        assert!(!Destination::Chats.lit(&app), "Chats goes dark beside it");
        app.show_starred = true;
        assert!(!lit(&app));
        assert!(Destination::Starred.lit(&app));
    }
}
