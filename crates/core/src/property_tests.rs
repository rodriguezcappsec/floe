//! Selective property tests for invariants with large input spaces.
//!
//! These complement deterministic unit and temporary-filesystem tests. Keep
//! examples with a clearer fixed input in their owning module instead.

#[cfg(unix)]
mod unix {
    use std::{
        ffi::{OsStr, OsString},
        os::unix::ffi::{OsStrExt, OsStringExt},
        path::PathBuf,
    };

    use proptest::prelude::*;

    use crate::{
        DirectoryEntry, DirectoryGrouping, DirectoryPlacement, DirectorySort, EntryKind,
        FolderFilterMode, FolderFilterPattern, NavigationState, SortColumn, SortDirection,
        ThumbnailState,
    };

    fn filename_bytes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(
            any::<u8>().prop_filter("Linux filename bytes exclude NUL and slash", |byte| {
                *byte != 0 && *byte != b'/'
            }),
            1..64,
        )
        .prop_filter(
            "filesystem entries exclude reserved dot components",
            |name| name.as_slice() != b"." && name.as_slice() != b"..",
        )
    }

    fn entry(name: Vec<u8>, index: usize) -> DirectoryEntry {
        let name = OsString::from_vec(name);
        let path = PathBuf::from("/property-fixture").join(&name);
        let kind = if index % 4 == 0 {
            EntryKind::Directory
        } else {
            EntryKind::RegularFile
        };

        DirectoryEntry::new(
            path,
            name,
            kind,
            (!matches!(kind, EntryKind::Directory)).then_some(index as u64),
            None,
            None,
            false,
            false,
            ThumbnailState::NotRequested,
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 128,
            max_shrink_iters: 10_000,
            ..ProptestConfig::default()
        })]

        #[test]
        fn property_linux_filename_identity_survives_lossy_presentation(raw in filename_bytes()) {
            let entry = entry(raw.clone(), 1);

            let _presentation_only = entry.display_name_lossy();

            prop_assert_eq!(entry.display_name().as_bytes(), raw.as_slice());
            prop_assert_eq!(
                entry.path().file_name().map(OsStr::as_bytes),
                Some(raw.as_slice())
            );
        }

        #[test]
        fn property_sort_is_deterministic_and_preserves_the_entry_multiset(
            raw_names in prop::collection::vec(filename_bytes(), 0..96),
            column_index in 0usize..SortColumn::ALL.len(),
            descending in any::<bool>(),
            directories_last in any::<bool>(),
            hidden_last in any::<bool>(),
            grouping_index in 0usize..3,
        ) {
            let mut first: Vec<_> = raw_names
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, name)| entry(name, index))
                .collect();
            let mut second = first.clone();
            let before = sorted_identities(&first);
            let grouping = match grouping_index {
                0 => DirectoryGrouping::None,
                1 => DirectoryGrouping::Type,
                _ => DirectoryGrouping::Extension,
            };
            let policy = DirectorySort::new(
                SortColumn::ALL[column_index],
                if descending {
                    SortDirection::Descending
                } else {
                    SortDirection::Ascending
                },
            )
            .with_directories(if directories_last {
                DirectoryPlacement::Last
            } else {
                DirectoryPlacement::First
            })
            .with_grouping(grouping)
            .with_hidden_last(hidden_last);

            policy.sort_entries(&mut first);
            policy.sort_entries(&mut second);

            prop_assert_eq!(ordered_identities(&first), ordered_identities(&second));
            prop_assert_eq!(sorted_identities(&first), before);
        }

        #[test]
        fn property_text_filter_handles_arbitrary_non_utf8_names_without_panicking(
            mut prefix in filename_bytes(),
            query in "[a-z0-9]{1,12}",
        ) {
            prefix.push(0xff);
            prefix.extend(query.to_ascii_uppercase().bytes());
            let name = OsString::from_vec(prefix);
            let pattern = FolderFilterPattern::compile(FolderFilterMode::Text, &query)
                .expect("generated query is within the documented bound");

            prop_assert!(pattern.matches(&name));
            prop_assert!(FolderFilterPattern::compile(FolderFilterMode::Text, "")
                .expect("empty filter is valid")
                .matches(&name));
        }

        #[test]
        fn property_navigation_back_forward_round_trip(
            components in prop::collection::vec("[a-zA-Z0-9_-]{1,20}", 0..48),
        ) {
            let initial = PathBuf::from("/property-navigation");
            let mut state = NavigationState::new(initial.clone());
            let destinations: Vec<_> = components
                .iter()
                .enumerate()
                .map(|(index, component)| initial.join(format!("{index}-{component}")))
                .collect();

            for destination in &destinations {
                prop_assert!(state.navigate_to(destination.clone()));
            }
            for _ in &destinations {
                prop_assert!(state.go_back());
            }
            prop_assert_eq!(state.current(), initial.as_path());
            prop_assert!(!state.can_go_back());

            for _ in &destinations {
                prop_assert!(state.go_forward());
            }
            prop_assert_eq!(
                state.current(),
                destinations.last().map_or(initial.as_path(), PathBuf::as_path)
            );
            prop_assert!(!state.can_go_forward());
        }
    }

    fn ordered_identities(entries: &[DirectoryEntry]) -> Vec<Vec<u8>> {
        entries
            .iter()
            .map(|entry| entry.path().as_os_str().as_bytes().to_vec())
            .collect()
    }

    fn sorted_identities(entries: &[DirectoryEntry]) -> Vec<Vec<u8>> {
        let mut identities = ordered_identities(entries);
        identities.sort();
        identities
    }
}
