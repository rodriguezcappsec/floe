use std::path::{Path, PathBuf};

/// Toolkit-independent location and history state for one browser view.
#[derive(Clone, Debug)]
pub struct NavigationState {
    current: PathBuf,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
}

impl NavigationState {
    pub fn new(initial: PathBuf) -> Self {
        Self {
            current: initial,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    pub fn current(&self) -> &Path {
        &self.current
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn can_go_parent(&self) -> bool {
        self.current.parent().is_some()
    }

    pub fn navigate_to(&mut self, destination: PathBuf) -> bool {
        if destination == self.current {
            return false;
        }
        self.back
            .push(std::mem::replace(&mut self.current, destination));
        self.forward.clear();
        true
    }

    pub fn go_back(&mut self) -> bool {
        let Some(destination) = self.back.pop() else {
            return false;
        };
        self.forward
            .push(std::mem::replace(&mut self.current, destination));
        true
    }

    pub fn go_forward(&mut self) -> bool {
        let Some(destination) = self.forward.pop() else {
            return false;
        };
        self.back
            .push(std::mem::replace(&mut self.current, destination));
        true
    }

    pub fn go_parent(&mut self) -> bool {
        let Some(parent) = self.current.parent().map(Path::to_path_buf) else {
            return false;
        };
        self.navigate_to(parent)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::NavigationState;

    #[test]
    fn navigation_has_predictable_back_and_forward_history() {
        let mut state = NavigationState::new(PathBuf::from("/home"));

        assert!(state.navigate_to(PathBuf::from("/home/projects")));
        assert!(state.navigate_to(PathBuf::from("/home/projects/floe")));
        assert!(state.go_back());
        assert_eq!(state.current(), PathBuf::from("/home/projects"));
        assert!(state.go_forward());
        assert_eq!(state.current(), PathBuf::from("/home/projects/floe"));
    }

    #[test]
    fn a_new_destination_clears_forward_history() {
        let mut state = NavigationState::new(PathBuf::from("/"));
        state.navigate_to(PathBuf::from("/one"));
        state.go_back();
        state.navigate_to(PathBuf::from("/two"));

        assert!(!state.can_go_forward());
    }

    #[test]
    fn parent_navigation_stops_at_root() {
        let mut state = NavigationState::new(PathBuf::from("/one/two"));
        assert!(state.go_parent());
        assert_eq!(state.current(), PathBuf::from("/one"));
        assert!(state.go_parent());
        assert_eq!(state.current(), PathBuf::from("/"));
        assert!(!state.go_parent());
    }
}
