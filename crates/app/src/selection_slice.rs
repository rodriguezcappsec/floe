//! A bounded view onto the application's authoritative directory selection.
//!
//! GTK's `GridView` cannot render a row-spanning group header. Grouped Grid
//! View therefore uses one virtualized grid per contiguous group. This model
//! lets every section address only its own entries while delegating selection
//! to the one `MultiSelection` owned by the browser.

use std::{cell::Cell, cell::RefCell, sync::OnceLock};

use gtk::{gio, glib, prelude::*, subclass::prelude::*};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SelectionSlice {
        pub source: OnceLock<gtk::MultiSelection>,
        pub start: Cell<u32>,
        pub len: Cell<u32>,
        pub visible_len: Cell<u32>,
        pub selection_handler: RefCell<Option<glib::SignalHandlerId>>,
        pub items_handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SelectionSlice {
        const NAME: &'static str = "FloeSelectionSlice";
        type Type = super::SelectionSlice;
        type Interfaces = (gio::ListModel, gtk::SelectionModel);
    }

    impl ObjectImpl for SelectionSlice {
        fn dispose(&self) {
            if let Some(source) = self.source.get() {
                if let Some(handler) = self.selection_handler.borrow_mut().take() {
                    source.disconnect(handler);
                }
                if let Some(handler) = self.items_handler.borrow_mut().take() {
                    source.disconnect(handler);
                }
            }
        }
    }

    impl gio::subclass::prelude::ListModelImpl for SelectionSlice {
        fn item_type(&self) -> glib::Type {
            glib::BoxedAnyObject::static_type()
        }

        fn n_items(&self) -> u32 {
            self.visible_len.get()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            (position < self.n_items())
                .then(|| self.start.get().saturating_add(position))
                .and_then(|position| self.source.get()?.item(position))
        }
    }

    impl gtk::subclass::prelude::SelectionModelImpl for SelectionSlice {
        fn selection_in_range(&self, position: u32, n_items: u32) -> gtk::Bitset {
            let Some(source) = self.source.get() else {
                return gtk::Bitset::new_empty();
            };
            let n_items = super::clamp_count(position, n_items, self.n_items());
            let selection =
                source.selection_in_range(self.start.get().saturating_add(position), n_items);
            selection.shift_right(self.start.get());
            selection
        }

        fn is_selected(&self, position: u32) -> bool {
            position < self.n_items()
                && self.source.get().is_some_and(|source| {
                    source.is_selected(self.start.get().saturating_add(position))
                })
        }

        fn select_all(&self) -> bool {
            self.source
                .get()
                .is_some_and(|source| source.select_range(self.start.get(), self.n_items(), false))
        }

        fn select_item(&self, position: u32, unselect_rest: bool) -> bool {
            position < self.n_items()
                && self.source.get().is_some_and(|source| {
                    source.select_item(self.start.get().saturating_add(position), unselect_rest)
                })
        }

        fn select_range(&self, position: u32, n_items: u32, unselect_rest: bool) -> bool {
            let n_items = super::clamp_count(position, n_items, self.n_items());
            n_items > 0
                && self.source.get().is_some_and(|source| {
                    source.select_range(
                        self.start.get().saturating_add(position),
                        n_items,
                        unselect_rest,
                    )
                })
        }

        fn set_selection(&self, selected: &gtk::Bitset, mask: &gtk::Bitset) -> bool {
            let Some(source) = self.source.get() else {
                return false;
            };
            let bounds = gtk::Bitset::new_range(0, self.n_items());
            let selected = selected.copy();
            selected.intersect(&bounds);
            selected.shift_left(self.start.get());
            let mask = mask.copy();
            mask.intersect(&bounds);
            mask.shift_left(self.start.get());
            source.set_selection(&selected, &mask)
        }

        fn unselect_all(&self) -> bool {
            self.source
                .get()
                .is_some_and(|source| source.unselect_range(self.start.get(), self.n_items()))
        }

        fn unselect_item(&self, position: u32) -> bool {
            position < self.n_items()
                && self.source.get().is_some_and(|source| {
                    source.unselect_item(self.start.get().saturating_add(position))
                })
        }

        fn unselect_range(&self, position: u32, n_items: u32) -> bool {
            let n_items = super::clamp_count(position, n_items, self.n_items());
            n_items > 0
                && self.source.get().is_some_and(|source| {
                    source.unselect_range(self.start.get().saturating_add(position), n_items)
                })
        }
    }
}

glib::wrapper! {
    pub struct SelectionSlice(ObjectSubclass<imp::SelectionSlice>)
        @implements gio::ListModel, gtk::SelectionModel;
}

impl SelectionSlice {
    pub fn new(source: &gtk::MultiSelection, start: u32, len: u32) -> Self {
        let slice: Self = glib::Object::new();
        let imp = slice.imp();
        imp.source
            .set(source.clone())
            .expect("selection slice source is initialized once");
        imp.start.set(start);
        imp.len.set(len);
        imp.visible_len
            .set(len.min(source.n_items().saturating_sub(start)));

        let weak_slice = slice.downgrade();
        let handler = source.connect_selection_changed(move |_, position, n_items| {
            let Some(slice) = weak_slice.upgrade() else {
                return;
            };
            let start = slice.imp().start.get();
            let len = slice.n_items();
            if let Some((local_position, local_count)) =
                selection_overlap(start, len, position, n_items)
            {
                slice.selection_changed(local_position, local_count);
            }
        });
        imp.selection_handler.replace(Some(handler));

        let weak_slice = slice.downgrade();
        let handler = source.connect_items_changed(move |_, _, _, _| {
            let Some(slice) = weak_slice.upgrade() else {
                return;
            };
            let old_len = slice.imp().visible_len.replace(0);
            if old_len > 0 {
                slice.items_changed(0, old_len, 0);
            }
        });
        imp.items_handler.replace(Some(handler));
        slice
    }

    pub fn start(&self) -> u32 {
        self.imp().start.get()
    }
}

fn clamp_count(position: u32, requested: u32, len: u32) -> u32 {
    requested.min(len.saturating_sub(position))
}

fn selection_overlap(
    slice_start: u32,
    slice_len: u32,
    changed_start: u32,
    changed_len: u32,
) -> Option<(u32, u32)> {
    let slice_end = slice_start.saturating_add(slice_len);
    let changed_end = changed_start.saturating_add(changed_len);
    let overlap_start = slice_start.max(changed_start);
    let overlap_end = slice_end.min(changed_end);
    (overlap_start < overlap_end).then(|| {
        (
            overlap_start.saturating_sub(slice_start),
            overlap_end - overlap_start,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_overlap_maps_global_changes_into_the_section() {
        assert_eq!(selection_overlap(10, 5, 12, 8), Some((2, 3)));
        assert_eq!(selection_overlap(10, 5, 0, 11), Some((0, 1)));
        assert_eq!(selection_overlap(10, 5, 15, 1), None);
        assert_eq!(selection_overlap(10, 5, 9, 1), None);
    }

    #[test]
    fn range_clamping_never_crosses_a_section_boundary() {
        assert_eq!(clamp_count(2, 99, 5), 3);
        assert_eq!(clamp_count(5, 1, 5), 0);
        assert_eq!(clamp_count(u32::MAX, u32::MAX, 5), 0);
    }
}
