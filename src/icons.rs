// ============================================================================
// icons.rs — Anti-Aliased Cairo Studio Vector Icons & Circular Controls
// ============================================================================

use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk::cairo::{Context, LineCap, LineJoin};
use gtk::prelude::*;
use gtk::{DrawingArea, EventControllerMotion, GestureClick};

use crate::theme::{Rgb, Theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconName {
    Prev,
    Play,
    Pause,
    Next,
    Stop,
    Eq,
    Volume,
    Search,
    AddFile,
    Folder,
    Trash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconRole {
    /// 40x40 circular transport control (Prev, Next, Stop)
    Standard,
    /// 50x50 glowing primary hero circle (Play / Pause)
    Hero,
    /// 42x42 accented studio control (EQ Studio)
    Accent,
    /// 36x36 compact circular toolbar icon (Add Files, Add Folder, Clear)
    Compact,
}

impl IconRole {
    pub fn pixel_size(self) -> i32 {
        match self {
            Self::Standard => 40,
            Self::Hero => 50,
            Self::Accent => 42,
            Self::Compact => 36,
        }
    }

    pub fn icon_draw_px(self) -> f64 {
        match self {
            Self::Standard => 18.0,
            Self::Hero => 23.0,
            Self::Accent => 20.0,
            Self::Compact => 17.0,
        }
    }
}

fn cairo_rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

/// Renders one of the 11 studio vector icons in a normalized `24x24` coordinate space.
pub fn draw_icon_24(cr: &Context, icon: IconName, fg: Rgb) {
    let (r, g, b) = fg.cairo_f64();
    cr.set_source_rgb(r, g, b);
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);

    match icon {
        IconName::Play => {
            cr.move_to(8.0, 5.2);
            cr.line_to(19.6, 12.0);
            cr.line_to(8.0, 18.8);
            cr.close_path();
            let _ = cr.fill();
        }
        IconName::Pause => {
            cairo_rounded_rect(cr, 6.5, 4.5, 4.2, 15.0, 1.8);
            let _ = cr.fill();
            cairo_rounded_rect(cr, 13.3, 4.5, 4.2, 15.0, 1.8);
            let _ = cr.fill();
        }
        IconName::Stop => {
            cairo_rounded_rect(cr, 5.5, 5.5, 13.0, 13.0, 2.6);
            let _ = cr.fill();
        }
        IconName::Prev => {
            cairo_rounded_rect(cr, 4.5, 5.0, 2.6, 14.0, 1.3);
            let _ = cr.fill();
            cr.move_to(19.5, 5.8);
            cr.line_to(19.5, 18.2);
            cr.line_to(9.6, 12.0);
            cr.close_path();
            let _ = cr.fill();
        }
        IconName::Next => {
            cr.move_to(4.5, 5.8);
            cr.line_to(4.5, 18.2);
            cr.line_to(14.4, 12.0);
            cr.close_path();
            let _ = cr.fill();
            cairo_rounded_rect(cr, 16.9, 5.0, 2.6, 14.0, 1.3);
            let _ = cr.fill();
        }
        IconName::Eq => {
            cr.set_line_width(2.0);
            for &x in &[6.0, 12.0, 18.0] {
                cr.move_to(x, 4.0);
                cr.line_to(x, 20.0);
            }
            let _ = cr.stroke();
            for &(x, y) in &[(6.0, 9.0), (12.0, 15.0), (18.0, 8.0)] {
                cr.arc(x, y, 2.6, 0.0, 2.0 * PI);
                let _ = cr.fill();
            }
        }
        IconName::Volume => {
            cr.move_to(11.0, 5.0);
            cr.line_to(6.5, 9.0);
            cr.line_to(3.0, 9.0);
            cr.line_to(3.0, 15.0);
            cr.line_to(6.5, 15.0);
            cr.line_to(11.0, 19.0);
            cr.close_path();
            let _ = cr.fill();

            cr.set_line_width(2.0);
            cr.new_sub_path();
            cr.arc(12.2, 12.0, 3.8, -0.68, 0.68);
            let _ = cr.stroke();

            cr.new_sub_path();
            cr.arc(12.2, 12.0, 7.2, -0.68, 0.68);
            let _ = cr.stroke();
        }
        IconName::Search => {
            cr.set_line_width(2.2);
            cr.arc(11.0, 11.0, 6.4, 0.0, 2.0 * PI);
            let _ = cr.stroke();
            cr.move_to(15.8, 15.8);
            cr.line_to(20.5, 20.5);
            let _ = cr.stroke();
        }
        IconName::AddFile => {
            cr.set_line_width(2.0);
            cr.arc(12.0, 12.0, 8.5, 0.0, 2.0 * PI);
            let _ = cr.stroke();
            cr.set_line_width(2.2);
            cr.move_to(12.0, 8.0);
            cr.line_to(12.0, 16.0);
            cr.move_to(8.0, 12.0);
            cr.line_to(16.0, 12.0);
            let _ = cr.stroke();
        }
        IconName::Folder => {
            cr.set_line_width(2.0);
            cr.move_to(3.5, 7.0);
            cr.line_to(5.0, 5.2);
            cr.line_to(9.6, 5.2);
            cr.line_to(11.8, 7.4);
            cr.line_to(19.5, 7.4);
            cr.line_to(20.5, 8.5);
            cr.line_to(20.5, 18.0);
            cr.line_to(19.2, 19.2);
            cr.line_to(4.8, 19.2);
            cr.line_to(3.5, 18.0);
            cr.close_path();
            let _ = cr.stroke();
        }
        IconName::Trash => {
            cr.set_line_width(2.0);
            cr.move_to(4.5, 6.5);
            cr.line_to(19.5, 6.5);
            cr.move_to(9.5, 6.5);
            cr.line_to(9.5, 4.3);
            cr.line_to(14.5, 4.3);
            cr.line_to(14.5, 6.5);
            cr.move_to(6.8, 6.5);
            cr.line_to(7.7, 19.4);
            cr.line_to(16.3, 19.4);
            cr.line_to(17.2, 6.5);
            let _ = cr.stroke();
        }
    }
}

// ============================================================================
// Circular Studio Icon Control (`IconButton`)
// ============================================================================

struct IconBtnState {
    icon: IconName,
    role: IconRole,
    theme: Theme,
    hovered: bool,
    pressed: bool,
}

#[derive(Clone)]
pub struct IconButton {
    area: DrawingArea,
    state: Rc<RefCell<IconBtnState>>,
}

impl IconButton {
    pub fn new(icon: IconName, role: IconRole, tooltip: &str, theme: &Theme) -> Self {
        let sz = role.pixel_size();
        let area = DrawingArea::builder()
            .content_width(sz)
            .content_height(sz)
            .tooltip_text(tooltip)
            .build();
        area.set_cursor_from_name(Some("pointer"));

        let state = Rc::new(RefCell::new(IconBtnState {
            icon,
            role,
            theme: theme.clone(),
            hovered: false,
            pressed: false,
        }));

        let state_draw = Rc::clone(&state);
        area.set_draw_func(move |_, cr, w, h| {
            let st = state_draw.borrow();
            let th = &st.theme;
            let sz = w.min(h) as f64;
            let center = sz / 2.0;
            let radius = center - 2.4;

            let (fill_col, stroke_col, stroke_alpha, stroke_w, fg_col, icon_delta) = match st.role {
                IconRole::Hero => {
                    if st.pressed {
                        (th.btn_bg, th.accent, 0.95, 2.2, th.accent, -1.0)
                    } else if st.hovered {
                        (th.accent, th.text_main, 0.75, 2.2, th.accent_fg, 1.0)
                    } else {
                        (th.accent, th.accent, 0.28, 2.6, th.accent_fg, 0.0)
                    }
                }
                IconRole::Accent => {
                    if st.pressed {
                        (th.card_bg, th.accent, 1.0, 2.0, th.accent, -1.0)
                    } else if st.hovered {
                        (th.accent, th.accent, 0.95, 2.0, th.accent_fg, 0.0)
                    } else {
                        (th.btn_bg, th.accent, 0.75, 1.6, th.accent, 0.0)
                    }
                }
                IconRole::Standard | IconRole::Compact => {
                    if st.pressed {
                        (th.accent, th.accent, 1.0, 1.8, th.accent_fg, -1.0)
                    } else if st.hovered {
                        (th.btn_bg, th.accent, 0.95, 1.8, th.accent, 0.0)
                    } else {
                        (th.card_bg, th.border, 0.95, 1.4, th.text_main, 0.0)
                    }
                }
            };

            let (fr, fg, fb) = fill_col.cairo_f64();
            cr.set_source_rgb(fr, fg, fb);
            cr.arc(center, center, radius, 0.0, 2.0 * PI);
            let _ = cr.fill_preserve();

            let (sr, sg, sb) = stroke_col.cairo_f64();
            cr.set_source_rgba(sr, sg, sb, stroke_alpha);
            cr.set_line_width(stroke_w);
            let _ = cr.stroke();

            let icon_px = st.role.icon_draw_px() + icon_delta;
            let offset = (sz - icon_px) / 2.0;
            let scale = icon_px / 24.0;

            let _ = cr.save();
            cr.translate(offset, offset);
            cr.scale(scale, scale);
            draw_icon_24(cr, st.icon, fg_col);
            let _ = cr.restore();
        });

        let motion = EventControllerMotion::new();
        {
            let st = Rc::clone(&state);
            let area_c = area.clone();
            motion.connect_enter(move |_, _, _| {
                st.borrow_mut().hovered = true;
                area_c.queue_draw();
            });
        }
        {
            let st = Rc::clone(&state);
            let area_c = area.clone();
            motion.connect_leave(move |_| {
                let mut b = st.borrow_mut();
                b.hovered = false;
                b.pressed = false;
                area_c.queue_draw();
            });
        }
        area.add_controller(motion);

        Self { area, state }
    }

    pub fn widget(&self) -> &DrawingArea {
        &self.area
    }

    pub fn set_icon(&self, icon: IconName) {
        let mut st = self.state.borrow_mut();
        if st.icon != icon {
            st.icon = icon;
            drop(st);
            self.area.queue_draw();
        }
    }

    pub fn set_theme(&self, theme: &Theme) {
        self.state.borrow_mut().theme = theme.clone();
        self.area.queue_draw();
    }

    pub fn on_click<F: Fn() + 'static>(&self, callback: F) {
        let click = GestureClick::new();
        {
            let st = Rc::clone(&self.state);
            let area_c = self.area.clone();
            click.connect_pressed(move |_, _, _, _| {
                st.borrow_mut().pressed = true;
                area_c.queue_draw();
            });
        }
        {
            let st = Rc::clone(&self.state);
            let area_c = self.area.clone();
            click.connect_released(move |_, _, _, _| {
                let was_pressed = st.borrow().pressed;
                st.borrow_mut().pressed = false;
                area_c.queue_draw();
                if was_pressed {
                    callback();
                }
            });
        }
        self.area.add_controller(click);
    }
}

// ============================================================================
// Static Vector Studio Icon (`StaticIcon`)
// ============================================================================

#[derive(Clone)]
pub struct StaticIcon {
    area: DrawingArea,
    theme: Rc<RefCell<Theme>>,
}

impl StaticIcon {
    pub fn new(icon: IconName, size: i32, theme: &Theme) -> Self {
        let area = DrawingArea::builder()
            .content_width(size)
            .content_height(size)
            .build();
        let theme_rc = Rc::new(RefCell::new(theme.clone()));
        let th_draw = Rc::clone(&theme_rc);

        area.set_draw_func(move |_, cr, w, h| {
            let th = th_draw.borrow();
            let sz = w.min(h) as f64;
            let scale = sz / 24.0;
            let _ = cr.save();
            cr.scale(scale, scale);
            draw_icon_24(cr, icon, th.accent);
            let _ = cr.restore();
        });

        Self {
            area,
            theme: theme_rc,
        }
    }

    pub fn widget(&self) -> &DrawingArea {
        &self.area
    }

    pub fn set_theme(&self, theme: &Theme) {
        *self.theme.borrow_mut() = theme.clone();
        self.area.queue_draw();
    }
}
