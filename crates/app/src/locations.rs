use std::{collections::HashSet, path::PathBuf};

use gtk::glib::{self, UserDirectory};

#[derive(Clone, Debug)]
pub struct Location {
    pub label: &'static str,
    pub icon_name: &'static str,
    pub path: PathBuf,
}

pub fn standard_locations() -> Vec<Location> {
    let mut locations = vec![Location {
        label: "Home",
        icon_name: "user-home-symbolic",
        path: glib::home_dir(),
    }];
    add_special(
        &mut locations,
        "Downloads",
        "folder-download-symbolic",
        UserDirectory::Downloads,
    );
    add_special(
        &mut locations,
        "Documents",
        "folder-documents-symbolic",
        UserDirectory::Documents,
    );
    add_special(
        &mut locations,
        "Pictures",
        "folder-pictures-symbolic",
        UserDirectory::Pictures,
    );

    let mut seen = HashSet::new();
    locations.retain(|location| seen.insert(location.path.clone()));
    locations
}

fn add_special(
    locations: &mut Vec<Location>,
    label: &'static str,
    icon_name: &'static str,
    directory: UserDirectory,
) {
    if let Some(path) = glib::user_special_dir(directory) {
        locations.push(Location {
            label,
            icon_name,
            path,
        });
    }
}
