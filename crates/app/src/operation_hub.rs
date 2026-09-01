//! Application-owned fan-out for the destructive job event stream.
//!
//! `JobManager::drain_events` is intentionally destructive.  Giving every Floe
//! window an independent drain would therefore either steal events or duplicate
//! conflict/terminal presentation.  This small main-thread coordinator is the
//! only drain and assigns each event to exactly one live window inbox.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    rc::Rc,
};

use floe_core::JobEvent;

use crate::state::ApplicationState;

const WINDOW_CAPACITY: usize = 16;
const INBOX_EVENT_CAPACITY: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WindowRuntimeId(u64);

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum RegisterError {
    #[error("the maximum {0} Floe windows are already open")]
    Capacity(usize),
    #[error("window runtime ID space exhausted")]
    IdExhausted,
}

#[derive(Default)]
struct HubState {
    order: Vec<WindowRuntimeId>,
    inboxes: HashMap<WindowRuntimeId, VecDeque<JobEvent>>,
    active: Option<WindowRuntimeId>,
}

pub struct OperationEventHub {
    application_state: Rc<ApplicationState>,
    next_id: Cell<u64>,
    state: RefCell<HubState>,
}

impl OperationEventHub {
    pub fn new(application_state: Rc<ApplicationState>) -> Rc<Self> {
        Rc::new(Self {
            application_state,
            next_id: Cell::new(1),
            state: RefCell::new(HubState::default()),
        })
    }

    pub fn register(&self) -> Result<WindowRuntimeId, RegisterError> {
        let mut state = self.state.borrow_mut();
        if state.order.len() >= WINDOW_CAPACITY {
            return Err(RegisterError::Capacity(WINDOW_CAPACITY));
        }
        let raw = self.next_id.get();
        let next = raw.checked_add(1).ok_or(RegisterError::IdExhausted)?;
        self.next_id.set(next);
        let id = WindowRuntimeId(raw);
        state.order.push(id);
        state.inboxes.insert(id, VecDeque::new());
        state.active = Some(id);
        Ok(id)
    }

    pub fn mark_active(&self, id: WindowRuntimeId) {
        let mut state = self.state.borrow_mut();
        if state.inboxes.contains_key(&id) {
            state.active = Some(id);
        }
    }

    pub fn unregister(&self, id: WindowRuntimeId) {
        let mut state = self.state.borrow_mut();
        let orphaned = state.inboxes.remove(&id).unwrap_or_default();
        state.order.retain(|candidate| *candidate != id);
        if state.active == Some(id) {
            state.active = state.order.last().copied();
        }
        let target = state.active.or_else(|| state.order.last().copied());
        if let Some(target) = target
            && let Some(inbox) = state.inboxes.get_mut(&target)
        {
            append_bounded(inbox, orphaned);
        }
    }

    pub fn owns_presentation(&self, id: WindowRuntimeId) -> bool {
        let state = self.state.borrow();
        state.active.or_else(|| state.order.last().copied()) == Some(id)
    }

    pub fn drain_for(&self, id: WindowRuntimeId) -> Vec<JobEvent> {
        let events = self.application_state.drain_job_events();
        let mut state = self.state.borrow_mut();
        let target = state
            .active
            .filter(|candidate| state.inboxes.contains_key(candidate))
            .or_else(|| state.order.last().copied());
        if let Some(target) = target
            && let Some(inbox) = state.inboxes.get_mut(&target)
        {
            append_bounded(inbox, events);
        }
        state
            .inboxes
            .get_mut(&id)
            .map(|inbox| inbox.drain(..).collect())
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn live_windows(&self) -> usize {
        self.state.borrow().order.len()
    }
}

fn append_bounded(inbox: &mut VecDeque<JobEvent>, events: impl IntoIterator<Item = JobEvent>) {
    for event in events {
        if inbox.len() == INBOX_EVENT_CAPACITY {
            inbox.pop_front();
        }
        inbox.push_back(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_23h_runtime_registers_one_active_owner_and_reassigns_on_close() {
        let state = Rc::new(ApplicationState::new_selection_mode().expect("application state"));
        let hub = OperationEventHub::new(state);
        let first = hub.register().expect("first window");
        let second = hub.register().expect("second window");
        assert!(!hub.owns_presentation(first));
        assert!(hub.owns_presentation(second));
        hub.mark_active(first);
        assert!(hub.owns_presentation(first));
        hub.unregister(first);
        assert!(hub.owns_presentation(second));
        assert_eq!(hub.live_windows(), 1);
    }

    #[test]
    fn phase_23h_runtime_enforces_window_capacity() {
        let state = Rc::new(ApplicationState::new_selection_mode().expect("application state"));
        let hub = OperationEventHub::new(state);
        for _ in 0..WINDOW_CAPACITY {
            hub.register().expect("bounded window");
        }
        assert_eq!(
            hub.register(),
            Err(RegisterError::Capacity(WINDOW_CAPACITY))
        );
    }

    #[test]
    fn phase_23h_close_keeps_application_state_and_survivor_registration_alive() {
        let state = Rc::new(ApplicationState::new_selection_mode().expect("application state"));
        let state_weak = Rc::downgrade(&state);
        let hub = OperationEventHub::new(Rc::clone(&state));
        let survivor = hub.register().expect("survivor");
        let closing = hub.register().expect("closing window");
        drop(state);

        hub.unregister(closing);
        assert!(hub.owns_presentation(survivor));
        assert!(state_weak.upgrade().is_some());
        let third = hub.register().expect("third window after close");
        assert!(hub.owns_presentation(third));
        assert_eq!(hub.live_windows(), 2);
    }
}
