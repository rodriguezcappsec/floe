//! GTK-independent policy for Floe's optional Vim-style browser navigation.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VimCommand {
    Previous,
    Next,
    First,
    Last,
    Parent,
    Child,
    Open,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VimModifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

pub fn command_for_input(
    enabled: bool,
    file_view_focus: bool,
    character: Option<char>,
    modifiers: VimModifiers,
) -> Option<VimCommand> {
    if !enabled || !file_view_focus || modifiers.control || modifiers.alt || modifiers.super_key {
        return None;
    }
    match (character?, modifiers.shift) {
        ('h', false) => Some(VimCommand::Parent),
        ('j', false) => Some(VimCommand::Next),
        ('k', false) => Some(VimCommand::Previous),
        ('l', false) => Some(VimCommand::Child),
        ('g', false) => Some(VimCommand::First),
        ('G', true) | ('G', false) => Some(VimCommand::Last),
        ('o', false) => Some(VimCommand::Open),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_11d_vim_policy_maps_only_reviewed_unmodified_file_view_keys() {
        let plain = VimModifiers::default();
        for (character, command) in [
            ('h', VimCommand::Parent),
            ('j', VimCommand::Next),
            ('k', VimCommand::Previous),
            ('l', VimCommand::Child),
            ('g', VimCommand::First),
            ('o', VimCommand::Open),
        ] {
            assert_eq!(
                command_for_input(true, true, Some(character), plain),
                Some(command)
            );
        }
        assert_eq!(
            command_for_input(
                true,
                true,
                Some('G'),
                VimModifiers {
                    shift: true,
                    ..plain
                }
            ),
            Some(VimCommand::Last)
        );
        assert_eq!(command_for_input(false, true, Some('j'), plain), None);
        assert_eq!(
            command_for_input(
                true,
                true,
                Some('j'),
                VimModifiers {
                    control: true,
                    ..plain
                }
            ),
            None
        );
    }

    #[test]
    fn phase_11d_vim_policy_never_handles_editable_or_dialog_focus() {
        assert_eq!(
            command_for_input(true, false, Some('j'), VimModifiers::default()),
            None
        );
    }

    #[test]
    fn phase_11d_vim_preferences_are_opt_in_migrated_and_round_trip() {
        let legacy = crate::preferences::ViewPreferences::parse("version=5\nview=list\n");
        assert!(!legacy.vim_mode);
        let mut enabled = legacy;
        enabled.vim_mode = true;
        let serialized = enabled.serialize();
        assert!(serialized.starts_with("version=13\n"));
        assert!(serialized.contains("vim-mode=true\n"));
        assert!(crate::preferences::ViewPreferences::parse(&serialized).vim_mode);
        assert!(!crate::preferences::ViewPreferences::parse("vim-mode=invalid\n").vim_mode);
    }

    #[test]
    fn phase_11d_vim_ui_is_registered_discoverable_and_non_color_only() {
        let definition = crate::command_registry::command("win.vim-mode").expect("Vim mode");
        assert_eq!(definition.name, "Vim Navigation Mode");
        assert!(definition.default_shortcuts.is_empty());
        assert!(definition.search_terms.contains(&"hjkl"));
        assert_eq!(crate::ui::VIM_MODE_ON_LABEL, "Vim On");
        assert_eq!(crate::ui::VIM_MODE_OFF_LABEL, "Vim Off");
        assert!(crate::ui::VIM_MODE_TOOLTIP.contains("h/j/k/l"));
    }
}
