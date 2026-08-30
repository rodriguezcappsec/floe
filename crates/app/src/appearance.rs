use std::cell::{Cell, RefCell};

use gtk::{gdk, prelude::*};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppearancePreset {
    Native,
    Glass,
    Frosted,
    Minimal,
    Compact,
}

#[derive(Clone, Copy, Debug)]
pub struct Appearance {
    pub preset: AppearancePreset,
    panel_radius: u16,
    window_gap: u16,
    window_opacity: f32,
    header_opacity: f32,
    panel_opacity: f32,
    view_opacity: f32,
    row_padding: u16,
    shadow_opacity: f32,
    sidebar_width: u16,
}

impl Appearance {
    pub fn from_environment_or(fallback: AppearancePreset) -> Self {
        let preset = std::env::var("FLOE_APPEARANCE")
            .ok()
            .and_then(|value| AppearancePreset::parse(&value))
            .unwrap_or(fallback);
        Self::for_preset(preset)
    }

    pub fn for_preset(preset: AppearancePreset) -> Self {
        match preset {
            AppearancePreset::Native => Self {
                preset,
                panel_radius: 0,
                window_gap: 0,
                window_opacity: 1.0,
                header_opacity: 1.0,
                panel_opacity: 1.0,
                view_opacity: 1.0,
                row_padding: 8,
                shadow_opacity: 0.0,
                sidebar_width: 176,
            },
            AppearancePreset::Glass => Self {
                preset,
                panel_radius: 18,
                window_gap: 16,
                window_opacity: 0.0,
                header_opacity: 0.72,
                panel_opacity: 0.78,
                view_opacity: 0.0,
                row_padding: 9,
                shadow_opacity: 0.16,
                sidebar_width: 168,
            },
            AppearancePreset::Frosted => Self {
                preset,
                panel_radius: 16,
                window_gap: 14,
                window_opacity: 0.84,
                header_opacity: 0.92,
                panel_opacity: 0.94,
                view_opacity: 0.20,
                row_padding: 9,
                shadow_opacity: 0.12,
                sidebar_width: 168,
            },
            AppearancePreset::Minimal => Self {
                preset,
                panel_radius: 8,
                window_gap: 8,
                window_opacity: 1.0,
                header_opacity: 1.0,
                panel_opacity: 1.0,
                view_opacity: 1.0,
                row_padding: 8,
                shadow_opacity: 0.0,
                sidebar_width: 160,
            },
            AppearancePreset::Compact => Self {
                preset,
                panel_radius: 10,
                window_gap: 8,
                window_opacity: 1.0,
                header_opacity: 1.0,
                panel_opacity: 0.98,
                view_opacity: 1.0,
                row_padding: 4,
                shadow_opacity: 0.08,
                sidebar_width: 152,
            },
        }
    }

    pub fn class_name(self) -> &'static str {
        self.preset.class_name()
    }

    pub fn translucent_window(self) -> bool {
        self.window_opacity < 1.0
    }

    pub fn sidebar_width(self) -> i32 {
        i32::from(self.sidebar_width)
    }

    fn css(self) -> String {
        format!(
            r#"
            .floe-window.{preset_class} {{
                background-color: alpha(@window_bg_color, {window_opacity});
                background-image: none;
            }}

            .floe-window.{preset_class} headerbar {{
                background-color: alpha(@headerbar_bg_color, {header_opacity});
                border-bottom-color: alpha(@borders, 0.42);
                box-shadow: inset 0 -1px alpha(@borders, 0.26);
            }}

            .{preset_class} .floe-workspace,
            .{preset_class} .floe-tab-strip,
            .{preset_class} .floe-tab-scroller {{
                background-color: transparent;
            }}

            .{preset_class} .floe-directory-list,
            .{preset_class} .floe-directory-grid,
            .{preset_class} .floe-miller-column-list {{
                background-color: alpha(@view_bg_color, {view_opacity});
            }}

            .floe-workspace {{
                padding: {gap}px;
            }}

            .floe-panel {{
                background-color: alpha(@card_bg_color, {opacity});
                border: 1px solid alpha(@borders, 0.58);
                border-radius: {radius}px;
                box-shadow: 0 4px 18px alpha(black, {shadow});
            }}

            .floe-sidebar {{
                padding: 0;
            }}

            .floe-workspace > separator {{
                min-width: {handle_width}px;
                background-color: transparent;
                background-image: linear-gradient(
                    to right,
                    transparent 47%,
                    alpha(@borders, 0.52) 48%,
                    alpha(@borders, 0.52) 52%,
                    transparent 53%
                );
            }}

            .floe-workspace > separator:hover {{
                background-color: alpha(@accent_bg_color, 0.08);
            }}

.floe-sidebar button {{
  border-radius: 10px;
}}

.floe-drop-target {{
  outline: 2px dashed alpha(@accent_bg_color, 0.92);
  outline-offset: -3px;
  background-color: alpha(@accent_bg_color, 0.14);
}}

            .floe-sidebar.sidebar-compact button {{
                min-height: 32px;
                padding: 2px 8px;
            }}

            .floe-sidebar.sidebar-balanced button {{
                min-height: 36px;
                padding: 4px 8px;
            }}

            .floe-sidebar.sidebar-comfortable button {{
                min-height: 40px;
                padding: 6px 10px;
            }}

            .floe-sidebar button.sidebar-icon-button {{
                min-width: 36px;
                min-height: 36px;
            }}

            .floe-sidebar.sidebar-balanced button.sidebar-icon-button {{
                min-width: 40px;
                min-height: 40px;
            }}

            .floe-sidebar.sidebar-comfortable button.sidebar-icon-button {{
                min-width: 44px;
                min-height: 44px;
            }}

            .floe-directory-list row {{
                padding: {row_padding}px 12px;
                border-radius: 8px;
            }}

            .floe-directory-list.view-compact row {{
                padding: 3px 8px;
                border-radius: 6px;
            }}

            .floe-directory-list.view-comfortable row {{
                padding: 7px 12px;
            }}

            .floe-directory-list.view-spacious row {{
                padding: 11px 16px;
                border-radius: 10px;
            }}

            .floe-directory-list row:hover {{
                background-color: alpha(@accent_bg_color, 0.08);
            }}

            .floe-directory-list row:selected {{
                background-color: alpha(@accent_bg_color, 0.20);
            }}

            .floe-directory-list row:focus-visible {{
                box-shadow: inset 0 0 0 2px alpha(@accent_color, 0.76);
            }}

            .floe-directory-grid {{
                padding: 8px;
            }}

            .floe-directory-grid.view-compact {{
                padding: 4px;
            }}

            .floe-directory-grid.view-spacious {{
                padding: 12px;
            }}

            .floe-directory-grid child {{
                border-radius: 10px;
            }}

            .floe-directory-grid child:hover {{
                background-color: alpha(@accent_bg_color, 0.08);
            }}

            .floe-directory-grid child:selected {{
                background-color: alpha(@accent_bg_color, 0.20);
            }}

            .floe-directory-grid child:focus-visible {{
                box-shadow: inset 0 0 0 2px alpha(@accent_color, 0.76);
            }}

            .floe-grid-cell {{
                padding: 6px;
            }}

            .floe-directory-grid.view-compact .floe-grid-cell {{
                padding: 3px;
            }}

            .floe-directory-grid.view-spacious .floe-grid-cell {{
                padding: 10px;
            }}

.floe-grid-name {{
  font-weight: 500;
}}

.floe-grid-group-label {{
  padding: 0 4px;
}}

.floe-grid-sections,
.floe-grid-section {{
  background-color: transparent;
}}

.floe-grid-section-header {{
  min-height: 28px;
  padding: 2px 8px;
  border-radius: 8px;
}}

.floe-grid-section-header:hover {{
  background-color: alpha(@accent_bg_color, 0.08);
}}

.floe-grid-section-header:focus-visible {{
  box-shadow: 0 0 0 2px alpha(@accent_bg_color, 0.76);
}}

.floe-grid-section-body {{
  padding-top: 0;
}}

.floe-entry-icon {{
  opacity: 0.90;
}}

.floe-entry-icon.floe-icon-folder,
.floe-entry-icon.floe-icon-media,
.floe-entry-icon.floe-icon-code {{
  opacity: 1;
}}

.floe-entry-icon.floe-icon-document,
.floe-entry-icon.floe-icon-archive {{
  opacity: 0.92;
}}

.floe-entry-icon.floe-icon-generic {{
  opacity: 0.72;
}}

.icon-style-phosphor .floe-entry-icon.floe-icon-folder {{
  color: #62b8e8;
}}

.icon-style-phosphor .floe-entry-icon.floe-icon-media {{
  color: #55b996;
}}

.icon-style-phosphor .floe-entry-icon.floe-icon-archive {{
  color: #d9a441;
}}

.icon-style-phosphor .floe-entry-icon.floe-icon-code {{
  color: #a68be8;
}}

.icon-style-phosphor .floe-entry-icon.floe-icon-document {{
  color: #6f9ee8;
}}

.icon-style-phosphor .floe-entry-icon.floe-icon-generic {{
  color: @window_fg_color;
}}

.floe-directory-list row:selected .floe-entry-icon,
.floe-directory-grid child:selected .floe-entry-icon {{
  opacity: 1;
}}

.floe-list-header {{
                padding: 7px 12px 6px;
            }}

            .floe-list-header label {{
                color: @dim_label_color;
                font-size: 0.85em;
                font-weight: 600;
            }}

            .floe-metadata-heading {{
                padding: 0 6px;
            }}

            .floe-metadata-column {{
                color: @dim_label_color;
                padding: 0 6px;
            }}

            .floe-metadata-column.numeric {{
                font-feature-settings: "tnum";
            }}

            .floe-group-label {{
                color: @dim_label_color;
                font-size: 0.82em;
                font-weight: 600;
                padding-right: 8px;
            }}

            .floe-sort-heading {{
                min-height: 32px;
                padding: 0;
                border-radius: 6px;
            }}

            .floe-sort-heading:hover {{
                background-color: alpha(@accent_bg_color, 0.08);
            }}

            .floe-sort-heading.active-sort {{
                background-color: alpha(@accent_bg_color, 0.12);
            }}

            .floe-thumbnail {{
                border-radius: 5px;
            }}

            .floe-entry-name {{
                font-weight: 500;
            }}

            .floe-entry-type,
            .floe-entry-size,
            .floe-entry-modified,
            .floe-status {{
                color: @dim_label_color;
            }}

            .floe-entry-size, .floe-entry-modified {{
                font-feature-settings: "tnum";
            }}

.floe-path {{
  font-weight: 600;
}}

.floe-tab-strip {{
  padding: 0 8px 1px 8px;
}}

.floe-tab-bar {{
  padding: 0;
}}

.floe-tab {{
  min-width: 72px;
  min-height: 24px;
  padding: 0 1px 0 4px;
  border-radius: 0;
}}

.floe-tab-target,
.floe-tab-target:checked {{
  min-width: 44px;
  min-height: 22px;
  padding: 0 3px;
  background: transparent;
  box-shadow: none;
  font-weight: 400;
}}

.floe-tab.active {{
  background-color: transparent;
  box-shadow: inset 0 -2px @accent_bg_color;
}}

.floe-tab-close {{
  min-width: 20px;
  min-height: 20px;
  padding: 0;
  opacity: 0;
}}

.floe-tab:hover .floe-tab-close,
.floe-tab-close:focus {{
  opacity: 0.88;
}}

.floe-tab-new {{
  min-width: 24px;
  min-height: 24px;
  padding: 0;
}}

.floe-active-pane {{
  border: 1px solid alpha(@window_fg_color, 0.18);
  border-radius: 10px;
}}

.floe-split-snapshot {{
  background: alpha(@view_bg_color, 0.38);
}}

.floe-location-hit-target {{
                min-height: 36px;
                padding: 2px 10px;
                border-radius: 10px;
            }}

            .operations-island {{
                min-width: 320px;
                padding: 12px;
                background-color: alpha(@card_bg_color, {island_opacity});
                border: 1px solid alpha(@borders, 0.68);
                border-radius: {island_radius}px;
                box-shadow: 0 6px 24px alpha(black, {island_shadow});
            }}

            .operations-island progressbar trough {{
                min-height: 6px;
                border-radius: 999px;
            }}

            .operations-island progressbar progress {{
                min-height: 6px;
                border-radius: 999px;
            }}

            .operations-island button.operation-icon-action {{
                min-width: 40px;
                min-height: 40px;
                border-radius: 999px;
            }}

            .operations-island button.operation-text-action {{
                min-width: 72px;
                min-height: 36px;
                border-radius: 999px;
            }}

            .appearance-native .floe-workspace {{ padding: 0; }}
            .appearance-native .floe-panel {{ border-width: 0; box-shadow: none; }}
            .appearance-minimal .floe-panel {{ box-shadow: none; }}
            "#,
            preset_class = self.class_name(),
            gap = self.window_gap,
            window_opacity = self.window_opacity,
            header_opacity = self.header_opacity,
            opacity = self.panel_opacity,
            view_opacity = self.view_opacity,
            radius = self.panel_radius,
            shadow = self.shadow_opacity,
            row_padding = self.row_padding,
            handle_width = self.window_gap.max(8),
            island_opacity = self.panel_opacity.max(0.92),
            island_radius = self.panel_radius.max(10),
            island_shadow = self.shadow_opacity.max(0.10),
        )
    }
}

impl AppearancePreset {
    pub const ALL: [Self; 5] = [
        Self::Native,
        Self::Glass,
        Self::Frosted,
        Self::Minimal,
        Self::Compact,
    ];

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Glass => "glass",
            Self::Frosted => "frosted",
            Self::Minimal => "minimal",
            Self::Compact => "compact",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Native => "Native",
            Self::Glass => "Glass",
            Self::Frosted => "Frosted",
            Self::Minimal => "Minimal",
            Self::Compact => "Compact",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "native" => Some(Self::Native),
            "glass" => Some(Self::Glass),
            "frosted" => Some(Self::Frosted),
            "minimal" => Some(Self::Minimal),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }

    const fn class_name(self) -> &'static str {
        match self {
            Self::Native => "appearance-native",
            Self::Glass => "appearance-glass",
            Self::Frosted => "appearance-frosted",
            Self::Minimal => "appearance-minimal",
            Self::Compact => "appearance-compact",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::from_persisted(value.trim().to_ascii_lowercase().as_str())
    }
}

pub struct AppearanceManager {
    provider: gtk::CssProvider,
    preset: Cell<AppearancePreset>,
    font_family: RefCell<Option<String>>,
    font_scale_percent: Cell<u16>,
    reduced_motion: Cell<bool>,
}

impl AppearanceManager {
    pub fn new(window: &gtk::Widget, preset: AppearancePreset) -> Self {
        let provider = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        let manager = Self {
            provider,
            preset: Cell::new(preset),
            font_family: RefCell::new(None),
            font_scale_percent: Cell::new(100),
            reduced_motion: Cell::new(false),
        };
        manager.apply(window, preset);
        manager
    }

    pub fn preset(&self) -> AppearancePreset {
        self.preset.get()
    }

    pub fn apply(&self, window: &gtk::Widget, preset: AppearancePreset) {
        let appearance = Appearance::for_preset(preset);
        self.provider
            .load_from_string(&self.complete_css(appearance));
        for candidate in AppearancePreset::ALL {
            window.remove_css_class(candidate.class_name());
        }
        window.add_css_class(preset.class_name());
        if appearance.translucent_window() {
            window.remove_css_class("background");
        } else {
            window.add_css_class("background");
        }
        self.preset.set(preset);
    }

    pub fn apply_accessibility(
        &self,
        window: &gtk::Widget,
        font_family: Option<&str>,
        font_scale_percent: u16,
        reduced_motion: bool,
    ) {
        self.font_family.replace(font_family.map(ToOwned::to_owned));
        self.font_scale_percent
            .set(font_scale_percent.clamp(75, 200));
        self.reduced_motion.set(reduced_motion);
        if reduced_motion {
            window.add_css_class("floe-reduced-motion");
        } else {
            window.remove_css_class("floe-reduced-motion");
        }
        self.apply(window, self.preset.get());
    }

    fn complete_css(&self, appearance: Appearance) -> String {
        let mut css = appearance.css();
        css.push_str(&accessibility_css(
            self.font_family.borrow().as_deref(),
            self.font_scale_percent.get(),
            self.reduced_motion.get(),
        ));
        css
    }
}

fn accessibility_css(
    font_family: Option<&str>,
    font_scale_percent: u16,
    reduced_motion: bool,
) -> String {
    let family = font_family.map(|family| {
        family
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\'', "\\'")
    });
    let family = family
        .as_deref()
        .map_or_else(|| "initial".to_owned(), |family| format!("\"{family}\""));
    let motion = if reduced_motion { "0s" } else { "initial" };
    format!(
        "\n.floe-window {{ font-family: {family}; font-size: {}%; }}\n\
         .floe-window.floe-reduced-motion * {{ transition-duration: {motion}; }}\n\
         .floe-window .floe-group-label {{ border: 1px solid alpha(@borders, 0.72); }}\n\
         .floe-window .floe-active-pane {{ outline: 1px solid alpha(@window_fg_color, 0.32); outline-offset: -1px; }}\n",
        font_scale_percent.clamp(75, 200)
    )
}

#[cfg(test)]
mod tests {
    use super::{Appearance, AppearancePreset};

    #[test]
    fn phase_20b2_accessibility_css_keeps_non_color_focus_and_group_cues() {
        let css = super::accessibility_css(Some("Inter"), 125, true);
        assert!(css.contains(".floe-group-label { border:"));
        assert!(css.contains(".floe-active-pane { outline:"));
        assert!(!css.contains("outline: 2px solid @accent_color"));
        assert!(css.contains("transition-duration: 0s"));
    }

    #[test]
    fn phase_20b3_tabs_are_compact_and_active_pane_frame_is_neutral() {
        let css = Appearance::for_preset(AppearancePreset::Native).css();
        assert!(css.contains("min-width: 72px"));
        assert!(css.contains("min-height: 24px"));
        assert!(css.contains("min-width: 20px"));
        assert!(css.contains("font-weight: 400"));
        assert!(css.contains("background-color: transparent"));
        assert!(css.contains("border: 1px solid alpha(@window_fg_color, 0.18)"));
        assert!(!css.contains("border: 2px solid alpha(@accent_bg_color, 0.72)"));
    }

    #[test]
    fn phase_0_appearance_names_are_trimmed_and_case_insensitive() {
        assert_eq!(
            AppearancePreset::parse(" glass "),
            Some(AppearancePreset::Glass)
        );
        assert_eq!(
            AppearancePreset::parse("FROSTED"),
            Some(AppearancePreset::Frosted)
        );
        assert_eq!(AppearancePreset::parse("unknown"), None);
    }

    #[test]
    fn appearance_preset_ids_labels_and_menu_order_are_stable() {
        assert_eq!(
            AppearancePreset::ALL.map(|preset| (preset.persisted(), preset.label())),
            [
                ("native", "Native"),
                ("glass", "Glass"),
                ("frosted", "Frosted"),
                ("minimal", "Minimal"),
                ("compact", "Compact"),
            ]
        );
        for preset in AppearancePreset::ALL {
            assert_eq!(
                AppearancePreset::from_persisted(preset.persisted()),
                Some(preset)
            );
        }
    }

    #[test]
    fn phase_0_glass_css_exposes_composited_layers() {
        let css = Appearance::for_preset(AppearancePreset::Glass).css();

        assert!(css.contains(".floe-window.appearance-glass"));
        assert!(css.contains("background-color: alpha(@window_bg_color, 0);"));
        assert!(css.contains("background-color: alpha(@headerbar_bg_color, 0.72);"));
        assert!(css.contains(".appearance-glass .floe-directory-list"));
        assert!(css.contains(".appearance-glass .floe-directory-grid"));
        assert!(css.contains(".appearance-glass .floe-miller-column-list"));
        assert!(css.contains("background-color: alpha(@view_bg_color, 0);"));
    }

    #[test]
    fn phase_0_frosted_is_more_opaque_than_glass() {
        let glass = Appearance::for_preset(AppearancePreset::Glass);
        let frosted = Appearance::for_preset(AppearancePreset::Frosted);

        assert!(glass.translucent_window());
        assert!(frosted.translucent_window());
        assert!(frosted.window_opacity > glass.window_opacity);
        assert!(frosted.header_opacity > glass.header_opacity);
        assert!(frosted.panel_opacity > glass.panel_opacity);
        assert!(frosted.view_opacity > glass.view_opacity);

        let css = frosted.css();
        assert!(css.contains(".floe-window.appearance-frosted"));
        assert!(css.contains("background-color: alpha(@window_bg_color, 0.84);"));
        assert!(css.contains("background-color: alpha(@view_bg_color, 0.2);"));
    }

    #[test]
    fn phase_0_non_translucent_presets_keep_opaque_window_and_views() {
        for preset in [
            AppearancePreset::Native,
            AppearancePreset::Minimal,
            AppearancePreset::Compact,
        ] {
            let appearance = Appearance::for_preset(preset);
            assert!(!appearance.translucent_window());
            assert_eq!(appearance.window_opacity, 1.0);
            assert_eq!(appearance.header_opacity, 1.0);
            assert_eq!(appearance.view_opacity, 1.0);
        }
    }
}
