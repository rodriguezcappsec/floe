use gtk::gdk;

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
    panel_opacity: f32,
    row_padding: u16,
    shadow_opacity: f32,
    floating_panels: bool,
    sidebar_width: u16,
    sidebar_min_width: u16,
}

impl Appearance {
    pub fn from_environment() -> Self {
        let preset = std::env::var("FLOE_APPEARANCE")
            .ok()
            .and_then(|value| AppearancePreset::parse(&value))
            .unwrap_or(AppearancePreset::Frosted);
        Self::for_preset(preset)
    }

    pub fn for_preset(preset: AppearancePreset) -> Self {
        match preset {
            AppearancePreset::Native => Self {
                preset,
                panel_radius: 0,
                window_gap: 0,
                panel_opacity: 1.0,
                row_padding: 8,
                shadow_opacity: 0.0,
                floating_panels: false,
                sidebar_width: 176,
                sidebar_min_width: 136,
            },
            AppearancePreset::Glass => Self {
                preset,
                panel_radius: 18,
                window_gap: 16,
                panel_opacity: 0.78,
                row_padding: 9,
                shadow_opacity: 0.16,
                floating_panels: true,
                sidebar_width: 168,
                sidebar_min_width: 136,
            },
            AppearancePreset::Frosted => Self {
                preset,
                panel_radius: 16,
                window_gap: 14,
                panel_opacity: 0.94,
                row_padding: 9,
                shadow_opacity: 0.12,
                floating_panels: true,
                sidebar_width: 168,
                sidebar_min_width: 136,
            },
            AppearancePreset::Minimal => Self {
                preset,
                panel_radius: 8,
                window_gap: 8,
                panel_opacity: 1.0,
                row_padding: 8,
                shadow_opacity: 0.0,
                floating_panels: true,
                sidebar_width: 160,
                sidebar_min_width: 128,
            },
            AppearancePreset::Compact => Self {
                preset,
                panel_radius: 10,
                window_gap: 8,
                panel_opacity: 0.98,
                row_padding: 4,
                shadow_opacity: 0.08,
                floating_panels: true,
                sidebar_width: 152,
                sidebar_min_width: 124,
            },
        }
    }

    pub fn class_name(self) -> &'static str {
        match self.preset {
            AppearancePreset::Native => "appearance-native",
            AppearancePreset::Glass => "appearance-glass",
            AppearancePreset::Frosted => "appearance-frosted",
            AppearancePreset::Minimal => "appearance-minimal",
            AppearancePreset::Compact => "appearance-compact",
        }
    }

    pub fn floating_panels(self) -> bool {
        self.floating_panels
    }

    pub fn sidebar_width(self) -> i32 {
        i32::from(self.sidebar_width)
    }

    pub fn sidebar_min_width(self) -> i32 {
        i32::from(self.sidebar_min_width)
    }

    pub fn install(self) {
        let css = format!(
            r#"
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
                padding: 12px;
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
                min-height: 38px;
                padding: 4px 10px;
                border-radius: 10px;
            }}

            .floe-directory-list row {{
                padding: {row_padding}px 12px;
                border-radius: 8px;
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

            .floe-list-header {{
                padding: 7px 12px 6px;
            }}

            .floe-list-header label {{
                color: @dim_label_color;
                font-size: 0.85em;
                font-weight: 600;
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

            .operations-island {{
                padding: 14px;
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

            .operations-island button {{
                min-width: 40px;
                min-height: 40px;
                border-radius: 999px;
            }}

            .appearance-native .floe-workspace {{ padding: 0; }}
            .appearance-native .floe-panel {{ border-width: 0; box-shadow: none; }}
            .appearance-minimal .floe-panel {{ box-shadow: none; }}
            "#,
            gap = self.window_gap,
            opacity = self.panel_opacity,
            radius = self.panel_radius,
            shadow = self.shadow_opacity,
            row_padding = self.row_padding,
            handle_width = self.window_gap.max(8),
            island_opacity = self.panel_opacity.max(0.92),
            island_radius = self.panel_radius.max(10),
            island_shadow = self.shadow_opacity.max(0.10),
        );

        let provider = gtk::CssProvider::new();
        provider.load_from_string(&css);
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }
}

impl AppearancePreset {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "native" => Some(Self::Native),
            "glass" => Some(Self::Glass),
            "frosted" => Some(Self::Frosted),
            "minimal" => Some(Self::Minimal),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }
}
