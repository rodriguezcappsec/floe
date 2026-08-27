use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use gtk::glib::{self, UserDirectory};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    pub label: &'static str,
    pub icon_name: &'static str,
    pub path: PathBuf,
}

#[derive(Clone, Copy)]
struct LocationDefinition {
    label: &'static str,
    icon_name: &'static str,
    directory: UserDirectory,
}

const XDG_LOCATION_DEFINITIONS: [LocationDefinition; 8] = [
    LocationDefinition {
        label: "Desktop",
        icon_name: "floe-phosphor-desktop-symbolic",
        directory: UserDirectory::Desktop,
    },
    LocationDefinition {
        label: "Documents",
        icon_name: "floe-phosphor-file-text-symbolic",
        directory: UserDirectory::Documents,
    },
    LocationDefinition {
        label: "Downloads",
        icon_name: "floe-phosphor-download-simple-symbolic",
        directory: UserDirectory::Downloads,
    },
    LocationDefinition {
        label: "Music",
        icon_name: "floe-phosphor-music-notes-symbolic",
        directory: UserDirectory::Music,
    },
    LocationDefinition {
        label: "Pictures",
        icon_name: "floe-phosphor-image-symbolic",
        directory: UserDirectory::Pictures,
    },
    LocationDefinition {
        label: "Public Share",
        icon_name: "floe-phosphor-users-symbolic",
        directory: UserDirectory::PublicShare,
    },
    LocationDefinition {
        label: "Templates",
        icon_name: "floe-phosphor-columns-symbolic",
        directory: UserDirectory::Templates,
    },
    LocationDefinition {
        label: "Videos",
        icon_name: "floe-phosphor-video-camera-symbolic",
        directory: UserDirectory::Videos,
    },
];

pub fn standard_locations() -> Vec<Location> {
    standard_locations_with(glib::home_dir(), glib::user_special_dir, |path| {
        path.is_dir()
    })
}

fn standard_locations_with(
    home: PathBuf,
    mut resolve_special: impl FnMut(UserDirectory) -> Option<PathBuf>,
    mut is_directory: impl FnMut(&Path) -> bool,
) -> Vec<Location> {
    let mut seen = HashSet::new();
    seen.insert(home.clone());

    let mut locations = vec![Location {
        label: "Home",
        icon_name: "floe-phosphor-house-symbolic",
        path: home,
    }];

    for definition in XDG_LOCATION_DEFINITIONS {
        let Some(path) = resolve_special(definition.directory) else {
            continue;
        };
        if is_directory(&path) && seen.insert(path.clone()) {
            locations.push(Location {
                label: definition.label,
                icon_name: definition.icon_name,
                path,
            });
        }
    }

    locations
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    #[test]
    fn phase_6k_standard_locations_include_every_existing_xdg_directory_in_order() {
        let home = PathBuf::from("/home/tester");
        let mappings = HashMap::from([
            (UserDirectory::Desktop, PathBuf::from("/places/desktop")),
            (UserDirectory::Documents, PathBuf::from("/places/documents")),
            (UserDirectory::Downloads, PathBuf::from("/places/downloads")),
            (UserDirectory::Music, PathBuf::from("/places/music")),
            (UserDirectory::Pictures, PathBuf::from("/places/pictures")),
            (UserDirectory::PublicShare, PathBuf::from("/places/public")),
            (UserDirectory::Templates, PathBuf::from("/places/templates")),
            (UserDirectory::Videos, PathBuf::from("/places/videos")),
        ]);
        let existing = mappings.values().cloned().collect::<HashSet<_>>();

        let locations = standard_locations_with(
            home.clone(),
            |directory| mappings.get(&directory).cloned(),
            |path| existing.contains(path),
        );

        assert_eq!(
            locations
                .iter()
                .map(|location| location.label)
                .collect::<Vec<_>>(),
            vec![
                "Home",
                "Desktop",
                "Documents",
                "Downloads",
                "Music",
                "Pictures",
                "Public Share",
                "Templates",
                "Videos",
            ]
        );
        assert_eq!(locations[0].path, home);
        for location in locations.iter().skip(1) {
            assert_eq!(
                mappings
                    .values()
                    .find(|path| *path == &location.path)
                    .map(PathBuf::as_path),
                Some(location.path.as_path())
            );
        }
    }

    #[test]
    fn phase_6k_standard_locations_omit_missing_unset_and_duplicate_paths() {
        let home = PathBuf::from("/home/tester");
        let duplicate = home.clone();
        let desktop = PathBuf::from("/places/desktop");
        let missing = PathBuf::from("/places/missing");

        let locations = standard_locations_with(
            home.clone(),
            |directory| match directory {
                UserDirectory::Desktop => Some(desktop.clone()),
                UserDirectory::Documents => Some(missing.clone()),
                UserDirectory::Downloads => Some(duplicate.clone()),
                UserDirectory::Music => Some(desktop.clone()),
                _ => None,
            },
            |path| path != missing,
        );

        assert_eq!(
            locations,
            vec![
                Location {
                    label: "Home",
                    icon_name: "floe-phosphor-house-symbolic",
                    path: home,
                },
                Location {
                    label: "Desktop",
                    icon_name: "floe-phosphor-desktop-symbolic",
                    path: desktop,
                },
            ]
        );
    }
}
