// ============================================================================
// theme.rs — Skin, Color Palette & Dynamic GTK 4 CSS Engine for GTKPlayer
// ============================================================================

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    FrostLight,
    DarkElegance,
    CyberpunkNeon,
    WarmVintage,
    MatrixEmerald,
    TokyoNight,
}

pub const ALL_THEMES: [ThemeMode; 6] = [
    ThemeMode::FrostLight,
    ThemeMode::DarkElegance,
    ThemeMode::CyberpunkNeon,
    ThemeMode::WarmVintage,
    ThemeMode::MatrixEmerald,
    ThemeMode::TokyoNight,
];

#[derive(Clone, Copy, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    pub fn rgba_css(self, alpha: f32) -> String {
        format!("rgba({}, {}, {}, {:.2})", self.0, self.1, self.2, alpha)
    }

    pub fn cairo_f64(self) -> (f64, f64, f64) {
        (
            self.0 as f64 / 255.0,
            self.1 as f64 / 255.0,
            self.2 as f64 / 255.0,
        )
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub mode: ThemeMode,
    pub name: &'static str,
    pub panel_bg: Rgb,
    pub card_bg: Rgb,
    pub btn_bg: Rgb,
    pub border: Rgb,
    pub accent: Rgb,
    pub accent_fg: Rgb,
    pub text_main: Rgb,
    pub text_muted: Rgb,
    pub spec_low: Rgb,
    pub spec_high: Rgb,
    pub spec_slot: Rgb,
}

impl Theme {
    pub fn get(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::FrostLight => Self {
                mode,
                name: "Frost Light",
                panel_bg: Rgb(236, 240, 245),
                card_bg: Rgb(255, 255, 255),
                btn_bg: Rgb(226, 232, 240),
                border: Rgb(203, 213, 225),
                accent: Rgb(37, 99, 235),
                accent_fg: Rgb(255, 255, 255),
                text_main: Rgb(15, 23, 42),
                text_muted: Rgb(100, 116, 139),
                spec_low: Rgb(37, 99, 235),
                spec_high: Rgb(124, 58, 237),
                spec_slot: Rgb(226, 232, 240),
            },
            ThemeMode::DarkElegance => Self {
                mode,
                name: "Dark Elegance",
                panel_bg: Rgb(18, 24, 38),
                card_bg: Rgb(26, 35, 52),
                btn_bg: Rgb(36, 48, 70),
                border: Rgb(48, 63, 90),
                accent: Rgb(56, 189, 248),
                accent_fg: Rgb(12, 18, 30),
                text_main: Rgb(241, 245, 249),
                text_muted: Rgb(148, 163, 184),
                spec_low: Rgb(35, 165, 245),
                spec_high: Rgb(180, 75, 235),
                spec_slot: Rgb(38, 50, 72),
            },
            ThemeMode::CyberpunkNeon => Self {
                mode,
                name: "Cyberpunk Neon",
                panel_bg: Rgb(11, 11, 20),
                card_bg: Rgb(20, 18, 36),
                btn_bg: Rgb(32, 26, 56),
                border: Rgb(58, 42, 96),
                accent: Rgb(0, 240, 255),
                accent_fg: Rgb(10, 10, 18),
                text_main: Rgb(250, 250, 255),
                text_muted: Rgb(217, 70, 239),
                spec_low: Rgb(0, 245, 255),
                spec_high: Rgb(255, 20, 180),
                spec_slot: Rgb(34, 28, 58),
            },
            ThemeMode::WarmVintage => Self {
                mode,
                name: "Warm Vintage Studio",
                panel_bg: Rgb(28, 22, 18),
                card_bg: Rgb(40, 31, 25),
                btn_bg: Rgb(58, 45, 36),
                border: Rgb(84, 65, 52),
                accent: Rgb(245, 158, 11),
                accent_fg: Rgb(24, 16, 10),
                text_main: Rgb(254, 243, 199),
                text_muted: Rgb(196, 160, 126),
                spec_low: Rgb(245, 158, 11),
                spec_high: Rgb(239, 68, 68),
                spec_slot: Rgb(58, 45, 36),
            },
            ThemeMode::MatrixEmerald => Self {
                mode,
                name: "Matrix Emerald",
                panel_bg: Rgb(10, 18, 14),
                card_bg: Rgb(16, 29, 22),
                btn_bg: Rgb(24, 44, 34),
                border: Rgb(38, 72, 56),
                accent: Rgb(16, 185, 129),
                accent_fg: Rgb(8, 16, 12),
                text_main: Rgb(220, 252, 231),
                text_muted: Rgb(110, 231, 183),
                spec_low: Rgb(16, 185, 129),
                spec_high: Rgb(163, 230, 53),
                spec_slot: Rgb(24, 44, 34),
            },
            ThemeMode::TokyoNight => Self {
                mode,
                name: "Tokyo Night",
                panel_bg: Rgb(26, 27, 38),
                card_bg: Rgb(36, 40, 59),
                btn_bg: Rgb(52, 59, 88),
                border: Rgb(65, 72, 104),
                accent: Rgb(122, 162, 247),
                accent_fg: Rgb(22, 22, 30),
                text_main: Rgb(192, 202, 245),
                text_muted: Rgb(169, 177, 214),
                spec_low: Rgb(122, 162, 247),
                spec_high: Rgb(187, 154, 247),
                spec_slot: Rgb(44, 49, 74),
            },
        }
    }

    pub fn is_dark(&self) -> bool {
        self.mode != ThemeMode::FrostLight
    }

    /// Interpolates the spectrum bar color for bar `index` out of `total`.
    pub fn bar_color(&self, index: usize, total: usize) -> Rgb {
        let t = index as f32 / total.max(1) as f32;
        let r = (self.spec_low.0 as f32 + t * (self.spec_high.0 as f32 - self.spec_low.0 as f32)) as u8;
        let g = (self.spec_low.1 as f32 + t * (self.spec_high.1 as f32 - self.spec_low.1 as f32)) as u8;
        let b = (self.spec_low.2 as f32 + t * (self.spec_high.2 as f32 - self.spec_low.2 as f32)) as u8;
        Rgb(r, g, b)
    }

    /// Generates the complete GTK 4 CSS stylesheet for the current Studio Skin.
    pub fn to_gtk_css(&self) -> String {
        let panel_bg = self.panel_bg.hex();
        let card_bg = self.card_bg.hex();
        let btn_bg = self.btn_bg.hex();
        let border = self.border.hex();
        let accent = self.accent.hex();
        let accent_soft = self.accent.rgba_css(0.75);
        let accent_fg = self.accent_fg.hex();
        let text_main = self.text_main.hex();
        let text_muted = self.text_muted.hex();
        let spec_slot = self.spec_slot.hex();

        let row_sel_bg = if self.is_dark() {
            self.accent.rgba_css(0.32)
        } else {
            "#5B8DC4".to_string()
        };
        let row_sel_fg = "#FFFFFF";

        format!(
            r#"
            window, .studio-window {{
                background-color: {panel_bg};
                color: {text_main};
                font-family: "Figtree", "Inter", "Noto Sans", "Segoe UI", sans-serif;
                font-size: 13px;
            }}

            menubar {{
                background-color: {btn_bg};
                background-image: none;
                color: {text_main};
                border-bottom: 1px solid {border};
                padding: 2px 6px;
                font-weight: 600;
            }}

            menubar > item {{
                background-color: transparent;
                background-image: none;
                padding: 4px 10px;
                border-radius: 5px;
                color: {text_main};
            }}

            menubar > item > label {{
                color: {text_main};
            }}

            menubar > item:hover, menubar > item:selected {{
                background-color: {accent};
                background-image: none;
                color: {accent_fg};
            }}

            menubar > item:hover > label, menubar > item:selected > label {{
                color: {accent_fg};
            }}

            popover.menu contents {{
                background-color: {card_bg};
                color: {text_main};
                border: 1px solid {border};
                border-radius: 8px;
                padding: 4px;
            }}

            popover.menu modelbutton {{
                padding: 5px 12px;
                border-radius: 5px;
                color: {text_main};
            }}

            popover.menu modelbutton:hover {{
                background-color: {accent};
                color: {accent_fg};
            }}

            frame.studio-section {{
                background-color: transparent;
                border: none;
            }}

            frame.studio-section > border {{
                border: 1px solid {border};
                border-radius: 6px;
            }}

            .section-header {{
                color: {accent};
                font-weight: 700;
                font-size: 13px;
                padding: 0 6px;
            }}

            .track-title {{
                color: {text_main};
                font-weight: 800;
                font-size: 20px;
            }}

            .track-artist {{
                color: {accent};
                font-weight: 600;
                font-size: 14px;
            }}

            .time-label {{
                color: {text_muted};
                font-weight: 600;
                font-size: 13px;
                font-variant-numeric: tabular-nums;
            }}

            .vol-label {{
                color: {text_main};
                font-weight: 600;
                font-size: 13px;
                font-variant-numeric: tabular-nums;
            }}

            .spec-caption {{
                color: {text_muted};
                font-weight: 600;
                font-size: 12.5px;
            }}

            .filter-label {{
                color: {text_main};
                font-weight: 600;
                font-size: 13px;
            }}

            entry.studio-filter {{
                background-color: {card_bg};
                color: {text_main};
                border: 1px solid {border};
                border-radius: 5px;
                min-height: 30px;
                padding: 2px 10px;
                font-weight: 600;
                font-size: 13px;
            }}

            entry.studio-filter:focus-within {{
                border-color: {accent};
                outline: none;
            }}

            scrolledwindow.playlist-scroll,
            scrolledwindow.playlist-scroll > viewport {{
                background-color: {card_bg};
                background-image: none;
                border: 1px solid {border};
                border-radius: 4px;
            }}

            listbox.studio-playlist,
            list.studio-playlist {{
                background-color: {card_bg};
                background-image: none;
                color: {text_main};
            }}

            listbox.studio-playlist > row,
            list.studio-playlist > row {{
                background-color: {card_bg};
                background-image: none;
                color: {text_main};
                padding: 5px 10px;
                border-radius: 0px;
                font-weight: 600;
                font-size: 13.5px;
            }}

            listbox.studio-playlist > row > label,
            list.studio-playlist > row > label {{
                color: {text_main};
            }}

            listbox.studio-playlist > row:hover,
            list.studio-playlist > row:hover {{
                background-color: {btn_bg};
                background-image: none;
            }}

            listbox.studio-playlist > row:selected,
            list.studio-playlist > row:selected {{
                background-color: {row_sel_bg};
                background-image: none;
                color: {row_sel_fg};
            }}

            listbox.studio-playlist > row:selected label,
            list.studio-playlist > row:selected label {{
                color: {row_sel_fg};
                font-weight: 700;
            }}

            scrollbar slider {{
                min-width: 6px;
                min-height: 24px;
                border-radius: 99px;
                background-color: {border};
            }}

            scale.studio-scale contents trough {{
                background-color: {spec_slot};
                border: 1px solid {border};
                border-radius: 99px;
                min-height: 6px;
                min-width: 6px;
            }}

            scale.studio-scale contents trough highlight {{
                background-color: {accent_soft};
                border-radius: 99px;
            }}

            scale.studio-scale contents trough slider {{
                background-color: {panel_bg};
                border: 1.5px solid {text_muted};
                border-radius: 99px;
                min-width: 16px;
                min-height: 16px;
                margin: -5px;
            }}

            scale.studio-scale contents trough slider:hover {{
                border-color: {accent};
                background-color: {card_bg};
            }}

            .status-bar {{
                background-color: {btn_bg};
                border-top: 1px solid {border};
                padding: 5px 12px;
            }}

            .status-label {{
                color: {text_main};
                font-weight: 600;
                font-size: 12.5px;
            }}

            button.preset-btn {{
                background-color: {btn_bg};
                background-image: none;
                color: {text_main};
                border: 1px solid {border};
                border-radius: 6px;
                padding: 5px 10px;
                font-weight: 700;
                font-size: 12px;
            }}

            button.preset-btn label,
            button.preset-btn > label {{
                color: {text_main};
            }}

            button.preset-btn:hover {{
                border-color: {accent};
                background-color: {card_bg};
                background-image: none;
            }}

            button.preset-btn:hover label,
            button.preset-btn:hover > label {{
                color: {text_main};
            }}

            button.preset-btn-active {{
                background-color: {accent};
                background-image: none;
                color: {accent_fg};
                border: 1px solid {accent};
                border-radius: 6px;
                padding: 5px 10px;
                font-weight: 700;
                font-size: 12px;
            }}

            button.preset-btn-active label,
            button.preset-btn-active > label {{
                color: {accent_fg};
            }}

            button.studio-primary-btn {{
                background-color: {accent};
                background-image: none;
                color: {accent_fg};
                border: 1px solid {accent};
                border-radius: 6px;
                padding: 6px 18px;
                font-weight: 700;
                font-size: 13px;
            }}

            button.studio-primary-btn label,
            button.studio-primary-btn > label {{
                color: {accent_fg};
            }}

            .eq-db-label {{
                color: {accent};
                font-weight: 700;
                font-size: 12px;
                font-variant-numeric: tabular-nums;
            }}

            .eq-freq-label {{
                color: {text_muted};
                font-weight: 600;
                font-size: 11.5px;
            }}
            "#
        )
    }
}
