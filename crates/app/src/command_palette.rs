//! Native bounded command palette over the Phase 11A registry.

use std::{cell::RefCell, cmp::Reverse, collections::VecDeque, rc::Rc};

use adw::prelude::*;
use gtk::gdk;
#[cfg(test)]
use gtk::gio;

use crate::command_registry::{
    self, CommandActionSource, CommandAvailability, CommandDefinition, CommandRisk, ResolvedCommand,
};

pub const PALETTE_QUERY_CAPACITY: usize = 128;
pub const PALETTE_RESULT_CAPACITY: usize = 64;
pub const PALETTE_RECENT_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaletteMatch {
    pub definition: &'static CommandDefinition,
    pub score: u16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecentCommands {
    actions: VecDeque<&'static str>,
}

impl RecentCommands {
    pub fn record(&mut self, action: &'static str) -> bool {
        let Some(definition) = command_registry::command(action) else {
            return false;
        };
        if !definition.searchable {
            return false;
        }
        self.actions.retain(|candidate| *candidate != action);
        self.actions.push_front(action);
        self.actions.truncate(PALETTE_RECENT_CAPACITY);
        true
    }

    fn rank(&self, action: &str) -> Option<usize> {
        self.actions
            .iter()
            .position(|candidate| *candidate == action)
    }

    #[cfg(test)]
    fn actions(&self) -> Vec<&'static str> {
        self.actions.iter().copied().collect()
    }
}

pub fn bounded_query(query: &str) -> String {
    query
        .trim()
        .chars()
        .take(PALETTE_QUERY_CAPACITY)
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn search_commands(query: &str, recent: &RecentCommands) -> Vec<PaletteMatch> {
    let query = bounded_query(query);
    let mut matches = command_registry::COMMANDS
        .iter()
        .filter(|definition| definition.searchable)
        .filter_map(|definition| {
            score_command(definition, &query, recent)
                .map(|score| PaletteMatch { definition, score })
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|matched| Reverse(matched.score));
    matches.truncate(PALETTE_RESULT_CAPACITY);
    matches
}

fn score_command(
    definition: &'static CommandDefinition,
    query: &str,
    recent: &RecentCommands,
) -> Option<u16> {
    if query.is_empty() {
        return Some(
            recent
                .rank(definition.action)
                .map_or(100, |rank| 1_000u16.saturating_sub(rank as u16)),
        );
    }
    let name = definition.name.to_lowercase();
    let action = definition.action_name().to_lowercase();
    if name == query || action == query {
        return Some(1_000);
    }
    if name.starts_with(query) || action.starts_with(query) {
        return Some(900);
    }
    if name.split_whitespace().any(|word| word.starts_with(query)) {
        return Some(825);
    }
    if name.contains(query) {
        return Some(750);
    }
    if definition.category.label().to_lowercase().contains(query) {
        return Some(650);
    }
    if definition
        .search_terms
        .iter()
        .any(|term| term.to_lowercase().contains(query))
    {
        return Some(575);
    }
    definition
        .description
        .to_lowercase()
        .contains(query)
        .then_some(400)
}

pub fn activate_command<A: CommandActionSource>(
    source: &A,
    definition: &'static CommandDefinition,
    recent: &mut RecentCommands,
) -> bool {
    let Some(action) = source.command_action(definition.action_name()) else {
        return false;
    };
    if !action.is_enabled() {
        return false;
    }
    action.activate(None);
    recent.record(definition.action)
}

#[derive(Clone)]
pub struct CommandPalette {
    inner: Rc<PaletteInner>,
}

struct PaletteInner {
    window: adw::ApplicationWindow,
    dialog: adw::Dialog,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
    status: gtk::Label,
    recent: RefCell<RecentCommands>,
    visible: RefCell<Vec<ResolvedCommand>>,
}

impl CommandPalette {
    pub fn new(window: &adw::ApplicationWindow) -> Self {
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search commands")
            .hexpand(true)
            .build();
        search.update_property(&[gtk::accessible::Property::Label("Search Floe commands")]);
        let status = gtk::Label::builder()
            .label("Type to search commands")
            .halign(gtk::Align::Start)
            .build();
        status.add_css_class("floe-status");
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .activate_on_single_click(false)
            .build();
        list.add_css_class("boxed-list");
        list.update_property(&[
            gtk::accessible::Property::Label("Command results"),
            gtk::accessible::Property::Description(
                "Use arrow keys to choose an available command and Enter to run it",
            ),
        ]);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(360)
            .vexpand(true)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&search);
        content.append(&status);
        content.append(&scroller);
        let dialog = adw::Dialog::builder()
            .title("Command Palette")
            .content_width(620)
            .content_height(520)
            .child(&content)
            .focus_widget(&search)
            .build();
        dialog.update_property(&[
            gtk::accessible::Property::Label("Command Palette"),
            gtk::accessible::Property::Description(
                "Search and run available Floe commands. Recent commands stay in memory only.",
            ),
        ]);
        let inner = Rc::new(PaletteInner {
            window: window.clone(),
            dialog,
            search,
            list,
            status,
            recent: RefCell::new(RecentCommands::default()),
            visible: RefCell::new(Vec::with_capacity(PALETTE_RESULT_CAPACITY)),
        });
        PaletteInner::wire(&inner);
        Self { inner }
    }

    pub fn present(&self) {
        self.inner.search.set_text("");
        self.inner.refresh();
        self.inner.dialog.present(Some(&self.inner.window));
        self.inner.search.grab_focus();
    }
}

impl PaletteInner {
    fn wire(this: &Rc<Self>) {
        let weak = Rc::downgrade(this);
        this.search.connect_search_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.refresh();
            }
        });
        let weak = Rc::downgrade(this);
        this.search.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.activate_first_available();
            }
        });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(this);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if key == gdk::Key::Down {
                if let Some(row) = this.list.row_at_index(0) {
                    this.list.select_row(Some(&row));
                    row.grab_focus();
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        this.search.add_controller(keys);
        let weak = Rc::downgrade(this);
        this.list.connect_row_activated(move |_, row| {
            if let Some(this) = weak.upgrade() {
                this.activate_index(row.index());
            }
        });
    }

    fn refresh(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let matches = search_commands(self.search.text().as_str(), &self.recent.borrow());
        let mut visible = Vec::with_capacity(matches.len());
        let mut unavailable = 0usize;
        for matched in matches {
            let resolved = command_registry::resolve(&self.window, matched.definition);
            let available = resolved.can_activate();
            unavailable += usize::from(!available);
            let shortcut = matched
                .definition
                .default_shortcuts
                .first()
                .copied()
                .unwrap_or("No shortcut");
            let subtitle = match resolved.availability {
                CommandAvailability::Enabled => format!(
                    "{} · {} · {shortcut}",
                    matched.definition.category.label(),
                    matched.definition.description
                ),
                CommandAvailability::Disabled => format!(
                    "Unavailable in the current context · {}",
                    matched.definition.description
                ),
                CommandAvailability::Missing => {
                    "Unavailable because its application action is missing".to_owned()
                }
            };
            let row = adw::ActionRow::builder()
                .title(matched.definition.name)
                .subtitle(&subtitle)
                .activatable(available)
                .sensitive(available)
                .build();
            row.update_property(&[
                gtk::accessible::Property::Label(matched.definition.name),
                gtk::accessible::Property::Description(&subtitle),
            ]);
            if matches!(
                matched.definition.risk,
                CommandRisk::ConfirmationRequired | CommandRisk::Irreversible
            ) {
                let warning = gtk::Image::from_icon_name("dialog-warning-symbolic");
                warning.set_tooltip_text(Some("This command requires confirmation"));
                row.add_suffix(&warning);
            }
            self.list.append(&row);
            visible.push(resolved);
        }
        *self.visible.borrow_mut() = visible;
        let total = self.visible.borrow().len();
        if total == 0 {
            self.status.set_label("No matching commands");
        } else if unavailable == 0 {
            self.status.set_label(&format!("{total} commands"));
        } else {
            self.status
                .set_label(&format!("{total} commands · {unavailable} unavailable now"));
        }
        self.status
            .update_property(&[gtk::accessible::Property::Label(
                self.status.text().as_str(),
            )]);
    }

    fn activate_first_available(&self) {
        let index = self
            .visible
            .borrow()
            .iter()
            .position(|command| command.can_activate());
        if let Some(index) = index.and_then(|index| i32::try_from(index).ok()) {
            self.activate_index(index);
        }
    }

    fn activate_index(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(resolved) = self.visible.borrow().get(index).copied() else {
            return;
        };
        if !resolved.can_activate() {
            return;
        }
        if activate_command(
            &self.window,
            resolved.definition,
            &mut self.recent.borrow_mut(),
        ) {
            self.dialog.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_11b_palette_search_is_ranked_metadata_only_and_bounded() {
        let recent = RecentCommands::default();
        let matches = search_commands("open with", &recent);
        assert_eq!(matches[0].definition.action, "win.open-with");
        assert!(
            search_commands("sha256", &recent)
                .iter()
                .any(|item| item.definition.action == "win.checksum")
        );
        assert!(
            search_commands("split view", &recent)
                .iter()
                .any(|item| item.definition.category.label() == "Split View")
        );
        assert!(search_commands(&"x".repeat(1_000), &recent).len() <= PALETTE_RESULT_CAPACITY);
        assert_eq!(
            bounded_query(&"A".repeat(1_000)).chars().count(),
            PALETTE_QUERY_CAPACITY
        );
    }

    #[test]
    fn phase_11b_palette_recent_is_memory_only_deduplicated_and_bounded() {
        let mut recent = RecentCommands::default();
        for definition in command_registry::COMMANDS
            .iter()
            .filter(|definition| definition.searchable)
            .take(PALETTE_RECENT_CAPACITY + 5)
        {
            assert!(recent.record(definition.action));
        }
        assert_eq!(recent.actions().len(), PALETTE_RECENT_CAPACITY);
        let newest = recent.actions()[0];
        assert!(recent.record(newest));
        assert_eq!(recent.actions()[0], newest);
        assert!(!recent.record("win.not-a-command"));
    }

    #[test]
    fn phase_11b_palette_activation_policy_requires_live_enabled_action() {
        use std::{cell::Cell, rc::Rc};

        let group = gio::SimpleActionGroup::new();
        let open = gio::SimpleAction::new("open", None);
        let activated = Rc::new(Cell::new(0));
        let activated_for_signal = Rc::clone(&activated);
        open.connect_activate(move |_, _| activated_for_signal.set(activated_for_signal.get() + 1));
        open.set_enabled(false);
        group.add_action(&open);
        let definition = command_registry::command("win.open").expect("Open");
        let mut recent = RecentCommands::default();
        assert!(!activate_command(&group, definition, &mut recent));
        assert!(recent.actions().is_empty());
        open.set_enabled(true);
        assert!(activate_command(&group, definition, &mut recent));
        assert_eq!(activated.get(), 1);
        assert_eq!(recent.actions(), ["win.open"]);
    }

    #[test]
    fn phase_11b_palette_ui_contract_is_keyboard_accessible_and_nonpersistent() {
        let palette = command_registry::command("win.command-palette").expect("palette command");
        assert_eq!(palette.default_shortcuts, ["<Control><Shift>p"]);
        assert!(!palette.searchable);
        assert_eq!(PALETTE_RECENT_CAPACITY, 16);
        assert_eq!(PALETTE_RESULT_CAPACITY, 64);
    }
}
