use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

pub const RECENT_LOCATION_CAPACITY: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Breadcrumb {
    path: PathBuf,
    label: OsString,
}

impl Breadcrumb {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn label(&self) -> &OsStr {
        &self.label
    }
}

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

    pub fn breadcrumbs(&self) -> Vec<Breadcrumb> {
        breadcrumbs_for(&self.current)
    }

    pub fn recent_locations(&self) -> Vec<PathBuf> {
        let mut seen = HashSet::new();
        std::iter::once(&self.current)
            .chain(self.back.iter().rev())
            .chain(self.forward.iter().rev())
            .filter(|path| seen.insert((*path).clone()))
            .take(RECENT_LOCATION_CAPACITY)
            .cloned()
            .collect()
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

pub fn breadcrumbs_for(path: &Path) -> Vec<Breadcrumb> {
    if !path.is_absolute() {
        return Vec::new();
    }
    let mut result = vec![Breadcrumb {
        path: PathBuf::from("/"),
        label: OsString::from("/"),
    }];
    let mut accumulated = PathBuf::from("/");
    for component in path.components().skip(1) {
        accumulated.push(component.as_os_str());
        result.push(Breadcrumb {
            path: accumulated.clone(),
            label: component.as_os_str().to_os_string(),
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use super::{NavigationState, RECENT_LOCATION_CAPACITY, breadcrumbs_for};

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

    #[test]
    fn phase_7g_breadcrumb_segments_keep_exact_paths_and_root() {
        let state = NavigationState::new(PathBuf::from("/home/floe/projects"));
        let crumbs = state.breadcrumbs();
        assert_eq!(crumbs.len(), 4);
        assert_eq!(crumbs[0].path(), PathBuf::from("/"));
        assert_eq!(crumbs[3].path(), PathBuf::from("/home/floe/projects"));
        assert_eq!(crumbs[3].label(), "projects");
    }

    #[cfg(unix)]
    #[test]
    fn phase_7g_breadcrumb_non_utf8_identity_never_comes_from_display_text() {
        let path = PathBuf::from(OsString::from_vec(b"/tmp/raw-\xff/child".to_vec()));
        let crumbs = breadcrumbs_for(&path);
        assert_eq!(crumbs.last().expect("child").path(), path);
        assert_eq!(crumbs[2].label().as_encoded_bytes(), b"raw-\xff");
    }

    #[test]
    fn phase_7g_recent_locations_are_newest_first_deduplicated_and_bounded() {
        let mut state = NavigationState::new(PathBuf::from("/start"));
        for index in 0..(RECENT_LOCATION_CAPACITY + 20) {
            assert!(state.navigate_to(PathBuf::from(format!("/place-{index}"))));
        }
        assert!(state.navigate_to(PathBuf::from("/place-4")));
        let recent = state.recent_locations();
        assert_eq!(recent.len(), RECENT_LOCATION_CAPACITY);
        assert_eq!(recent[0], PathBuf::from("/place-4"));
        assert_eq!(
            recent
                .iter()
                .filter(|path| *path == &PathBuf::from("/place-4"))
                .count(),
            1
        );
    }
}
