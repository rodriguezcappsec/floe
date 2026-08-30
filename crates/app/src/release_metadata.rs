//! Phase 21B release identity and package-data consistency contracts.

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs, path::Path};

    use crate::{application::APPLICATION_ID, iconography::APPLICATION_ICON_NAME};

    const RELEASE_ID: &str = "io.github.rodriguezcappsec.Floe";

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("application crate belongs to repository workspace")
    }

    fn read(relative: &str) -> String {
        fs::read_to_string(repository_root().join(relative))
            .unwrap_or_else(|error| panic!("could not read {relative}: {error}"))
    }

    #[test]
    fn phase_21b_release_metadata_identity_mime_and_license_are_consistent() {
        assert_eq!(APPLICATION_ID, RELEASE_ID);
        assert_eq!(APPLICATION_ICON_NAME, RELEASE_ID);
        assert_eq!(env!("CARGO_BIN_NAME"), "floe");
        assert_eq!(env!("CARGO_PKG_LICENSE"), "LicenseRef-proprietary");

        let desktop = read("data/io.github.rodriguezcappsec.Floe.desktop");
        assert!(desktop.contains("Exec=floe %u\n"));
        assert!(desktop.contains(&format!("Icon={RELEASE_ID}\n")));
        assert!(desktop.contains("MimeType=inode/directory;\n"));
        assert_eq!(desktop.matches("MimeType=").count(), 1);
        assert!(desktop.contains("DBusActivatable=false\n"));

        let metainfo = read("data/io.github.rodriguezcappsec.Floe.metainfo.xml");
        assert!(metainfo.contains(&format!("<id>{RELEASE_ID}</id>")));
        assert!(metainfo.contains(&format!(
            "<launchable type=\"desktop-id\">{RELEASE_ID}.desktop</launchable>"
        )));
        assert!(metainfo.contains("<binary>floe</binary>"));
        assert!(metainfo.contains("<project_license>LicenseRef-proprietary</project_license>"));
        assert!(metainfo.contains("It is not a sandbox, encrypted vault"));

        let resources = read("crates/app/resources/floe.gresource.xml");
        assert!(resources.contains("prefix=\"/io/github/rodriguezcappsec/Floe\""));
        assert!(resources.contains(&format!("icons/512x512/apps/{RELEASE_ID}.png")));
    }

    #[test]
    fn phase_21b_release_metadata_icon_and_manifest_are_bounded_exact_assets() {
        let icon = repository_root()
            .join("data/icons/hicolor/512x512/apps")
            .join(format!("{RELEASE_ID}.png"));
        assert_eq!(
            image::image_dimensions(icon).expect("decode package icon"),
            (512, 512)
        );

        let manifest = read("packaging/install-manifest.txt");
        let mut destinations = HashSet::new();
        let mut entries = 0;
        for line in manifest
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let fields = line.split('|').collect::<Vec<_>>();
            assert_eq!(fields.len(), 3, "manifest row must have three fields");
            assert!(!fields[0].starts_with('/'));
            assert!(!fields[1].starts_with('/'));
            assert!(!fields[1].split('/').any(|component| component == ".."));
            assert!(matches!(fields[2], "0644" | "0755"));
            if fields[0] != "target/release/floe" {
                assert!(repository_root().join(fields[0]).exists());
            }
            assert!(destinations.insert(fields[1]));
            entries += 1;
        }
        assert_eq!(entries, 20);
        assert!(destinations.contains("bin/floe"));
        assert!(
            destinations.contains("share/applications/io.github.rodriguezcappsec.Floe.desktop")
        );
        assert!(destinations.contains("share/doc/floe/README.md"));
        assert!(destinations.contains("share/doc/floe/SECURITY.md"));
        assert!(destinations.contains("share/doc/floe/docs/GETTING_STARTED.md"));
        assert!(destinations.contains("share/doc/floe/docs/DEBUGGING.md"));
    }
}
