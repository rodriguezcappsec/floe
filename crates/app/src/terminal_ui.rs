//! Native preferred-terminal chooser over the reviewed provider registry.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;

use crate::terminal::{
    TERMINAL_PROVIDERS, TerminalAvailability, TerminalProviderId, provider_choices,
};

type PreferenceCallback = Box<dyn Fn(Option<TerminalProviderId>)>;

#[derive(Clone)]
pub struct TerminalChooser {
    inner: Rc<TerminalChooserInner>,
}

struct TerminalChooserInner {
    window: adw::ApplicationWindow,
    dialog: adw::Dialog,
    dropdown: gtk::DropDown,
    status: gtk::Label,
    availability: RefCell<Vec<TerminalAvailability>>,
    on_change: RefCell<Option<PreferenceCallback>>,
    ignore_change: Cell<bool>,
}

impl TerminalChooser {
    pub fn new(window: &adw::ApplicationWindow) -> Self {
        let dialog = adw::Dialog::builder()
            .title("Preferred Terminal")
            .content_width(520)
            .content_height(280)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_margin_start(24);
        content.set_margin_end(24);
        let heading = gtk::Label::builder()
            .label("Preferred Terminal")
            .xalign(0.0)
            .css_classes(["title-2"])
            .build();
        let explanation = gtk::Label::builder()
            .label("Automatic uses the first available reviewed terminal. If an explicit preference is unavailable, Floe reports the fallback it uses.")
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        let dropdown = gtk::DropDown::new(None::<gtk::StringList>, None::<gtk::Expression>);
        dropdown.set_hexpand(true);
        dropdown.update_property(&[gtk::accessible::Property::Label(
            "Preferred terminal application",
        )]);
        let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
        status.update_property(&[gtk::accessible::Property::Label(
            "Terminal provider availability",
        )]);
        let close = gtk::Button::with_label("Close");
        close.set_halign(gtk::Align::End);
        content.append(&heading);
        content.append(&explanation);
        content.append(&dropdown);
        content.append(&status);
        content.append(&close);
        dialog.set_child(Some(&content));

        let inner = Rc::new(TerminalChooserInner {
            window: window.clone(),
            dialog,
            dropdown,
            status,
            availability: RefCell::new(Vec::new()),
            on_change: RefCell::new(None),
            ignore_change: Cell::new(false),
        });
        let weak = Rc::downgrade(&inner);
        inner.dropdown.connect_selected_notify(move |dropdown| {
            if let Some(inner) = weak.upgrade() {
                inner.selection_changed(dropdown.selected());
            }
        });
        let dialog_for_close = inner.dialog.clone();
        close.connect_clicked(move |_| {
            dialog_for_close.close();
        });
        Self { inner }
    }

    pub fn present<F>(
        &self,
        preferred: Option<TerminalProviderId>,
        availability: Vec<TerminalAvailability>,
        on_change: F,
    ) where
        F: Fn(Option<TerminalProviderId>) + 'static,
    {
        let choices = provider_choices(&availability);
        let mut labels = vec!["Automatic (first available)".to_owned()];
        labels.extend(choices.iter().map(|choice| {
            format!(
                "{} — {}",
                choice.id.definition().name,
                if choice.available {
                    "Available"
                } else {
                    "Not installed"
                }
            )
        }));
        let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
        self.inner.ignore_change.set(true);
        self.inner
            .dropdown
            .set_model(Some(&gtk::StringList::new(&label_refs)));
        let selected = preferred
            .and_then(|id| {
                TERMINAL_PROVIDERS
                    .iter()
                    .position(|provider| provider.id == id)
            })
            .and_then(|index| u32::try_from(index + 1).ok())
            .unwrap_or(0);
        self.inner.dropdown.set_selected(selected);
        self.inner.ignore_change.set(false);
        *self.inner.availability.borrow_mut() = choices;
        *self.inner.on_change.borrow_mut() = Some(Box::new(on_change));
        self.inner.update_status(selected);
        self.inner.dialog.present(Some(&self.inner.window));
        self.inner.dropdown.grab_focus();
    }
}

impl TerminalChooserInner {
    fn selection_changed(&self, selected: u32) {
        if self.ignore_change.get() {
            return;
        }
        let preferred = usize::try_from(selected)
            .ok()
            .and_then(|selected| selected.checked_sub(1))
            .and_then(|index| TERMINAL_PROVIDERS.get(index))
            .map(|provider| provider.id);
        if let Some(callback) = self.on_change.borrow().as_ref() {
            callback(preferred);
        }
        self.update_status(selected);
    }

    fn update_status(&self, selected: u32) {
        let availability = self.availability.borrow();
        let installed = availability.iter().filter(|item| item.available).count();
        if selected == 0 {
            self.status.set_label(&format!(
                "Automatic selection · {installed} reviewed terminal provider(s) available"
            ));
            return;
        }
        let selected = usize::try_from(selected - 1).ok();
        match selected.and_then(|index| availability.get(index)) {
            Some(choice) if choice.available => self
                .status
                .set_label("This terminal is available and will be used directly."),
            Some(_) => self.status.set_label(
                "This terminal is not installed. Floe will report and use the first available reviewed fallback.",
            ),
            None => self.status.set_label("No terminal preference selected."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_11e_terminal_ui_is_bounded_registered_and_accessible() {
        assert_eq!(TERMINAL_PROVIDERS.len(), 9);
        assert!(crate::command_registry::command("win.open-terminal").is_some());
        assert!(crate::command_registry::command("win.terminal-preferences").is_some());
        assert!(
            TERMINAL_PROVIDERS
                .iter()
                .all(|provider| !provider.name.trim().is_empty())
        );
    }
}
