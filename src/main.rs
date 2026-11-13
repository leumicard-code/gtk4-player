// ============================================================================
// main.rs — GTKPlayer (Audio Engine V10 — Pure GTK 4 + GStreamer 10-Band DSP)
// ============================================================================

#[allow(dead_code)]
mod audio;
mod config;
mod icons;
mod theme;

use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk::cairo::Context;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, DrawingArea, Entry,
    FileDialog, FileFilter, Frame, GestureClick, Label, ListBox, ListBoxRow, Orientation, Scale,
    ScrolledWindow, SelectionMode, Window,
};
use walkdir::WalkDir;

use crate::audio::{ALL_PRESETS, AudioEngine, EQ_FREQUENCIES, EngineEvent};
use crate::config::{SavedTrack, UserConfig};
use crate::icons::{IconButton, IconName, IconRole, StaticIcon};
use crate::theme::{ALL_THEMES, Theme, ThemeMode};

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus"];
const SPECTRUM_BARS: usize = 64;

#[derive(Clone, Debug)]
struct PlaylistItem {
    path: PathBuf,
    title: String,
    artist: String,
}

struct AppState {
    playlist: Vec<PlaylistItem>,
    visible_indices: Vec<usize>,
    filter_query: String,
    current_idx: Option<usize>,
    is_playing: bool,
}

#[derive(Clone)]
struct SpectrumState {
    bars: Vec<f32>,
    peaks: Vec<f32>,
}

impl SpectrumState {
    fn new(count: usize) -> Self {
        Self {
            bars: vec![0.0; count],
            peaks: vec![0.0; count],
        }
    }

    fn update_from_fft(&mut self, fft_bins: &[f32]) {
        if fft_bins.is_empty() {
            return;
        }
        let usable_bins = (fft_bins.len() * 3 / 4).max(1);
        let count = self.bars.len();

        for i in 0..count {
            let t0 = (i as f32 / count as f32).powf(1.65);
            let t1 = ((i + 1) as f32 / count as f32).powf(1.65);

            let b0 = ((t0 * usable_bins as f32) as usize).min(usable_bins - 1);
            let b1 = ((t1 * usable_bins as f32).ceil() as usize).clamp(b0 + 1, usable_bins);

            let mut peak_bin = 0.0f32;
            for &v in &fft_bins[b0..b1] {
                peak_bin = peak_bin.max(v);
            }

            let presence = 1.0 + 0.35 * (i as f32 / count as f32);
            let target = (peak_bin * presence).clamp(0.0, 1.0);

            if target > self.bars[i] {
                self.bars[i] = self.bars[i] * 0.25 + target * 0.75;
            } else {
                self.bars[i] = (self.bars[i] * 0.80).max(0.0);
            }

            if self.bars[i] >= self.peaks[i] {
                self.peaks[i] = self.bars[i];
            } else {
                self.peaks[i] = (self.peaks[i] - 0.018).max(self.bars[i]);
            }
        }
    }

    fn decay(&mut self) -> bool {
        let mut active = false;
        for i in 0..self.bars.len() {
            if self.bars[i] > 0.004 || self.peaks[i] > 0.004 {
                active = true;
                self.bars[i] *= 0.82;
                self.peaks[i] = (self.peaks[i] - 0.022).max(self.bars[i]);
            } else {
                self.bars[i] = 0.0;
                self.peaks[i] = 0.0;
            }
        }
        active
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

fn make_section_frame(title: &str) -> Frame {
    let lbl = Label::new(Some(title));
    lbl.add_css_class("section-header");
    let frame = Frame::new(None);
    frame.set_label_widget(Some(&lbl));
    frame.add_css_class("studio-section");
    frame
}

fn format_time(sec: u64) -> String {
    format!("{}:{:02}", sec / 60, sec % 60)
}

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| AUDIO_EXTENSIONS.iter().any(|&a| a.eq_ignore_ascii_case(ext)))
        .unwrap_or(false)
}

fn parse_stem_title_artist(path: &Path) -> (String, String) {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Track")
        .trim();
    if let Some((artist, title)) = stem.split_once(" - ") {
        (title.trim().to_string(), artist.trim().to_string())
    } else {
        (stem.to_string(), String::new())
    }
}

fn collect_cli_audio_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg.starts_with('-') {
            continue;
        }
        let p = PathBuf::from(arg);
        if p.is_file() && is_audio_file(&p) {
            out.push(p);
        } else if p.is_dir() {
            let mut dir_files: Vec<PathBuf> = WalkDir::new(p)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file() && is_audio_file(e.path()))
                .map(|e| e.into_path())
                .collect();
            dir_files.sort();
            out.extend(dir_files);
        }
    }
    out
}

fn sync_playlist_to_config(st: &AppState, user_cfg: &Rc<RefCell<UserConfig>>) {
    let mut cfg = user_cfg.borrow_mut();
    cfg.playlist = st
        .playlist
        .iter()
        .map(|item| SavedTrack {
            path: item.path.clone(),
            title: item.title.clone(),
            artist: item.artist.clone(),
        })
        .collect();
    cfg.current_idx = st.current_idx;
    cfg.save();
}

fn rebuild_playlist_listbox(st: &mut AppState, list_box: &ListBox) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    st.visible_indices.clear();

    let query = st.filter_query.trim().to_lowercase();
    let mut selected_row: Option<ListBoxRow> = None;

    for (idx, item) in st.playlist.iter().enumerate() {
        if !query.is_empty()
            && !item.title.to_lowercase().contains(&query)
            && !item.artist.to_lowercase().contains(&query)
        {
            continue;
        }
        st.visible_indices.push(idx);
        let is_cur = st.current_idx == Some(idx);
        let prefix = if is_cur { "▶ " } else { "  " };
        let line = if item.artist.is_empty() {
            format!("{}{:02}. {}", prefix, idx + 1, item.title)
        } else {
            format!("{}{:02}. {} — {}", prefix, idx + 1, item.title, item.artist)
        };

        let lbl = Label::new(Some(&line));
        lbl.set_halign(Align::Start);
        lbl.set_xalign(0.0);
        lbl.set_ellipsize(pango::EllipsizeMode::End);

        let row = ListBoxRow::new();
        row.set_child(Some(&lbl));
        list_box.append(&row);

        if is_cur {
            selected_row = Some(row);
        }
    }

    if let Some(ref row) = selected_row {
        list_box.select_row(Some(row));
    }
}

// ============================================================================
// Equalizer & DSP Studio Window (Single-Instance + Skin-Aware)
// ============================================================================

fn open_equalizer_window(
    parent: &ApplicationWindow,
    eq_win_slot: &Rc<RefCell<Option<Window>>>,
    engine: &Rc<RefCell<AudioEngine>>,
    user_cfg: &Rc<RefCell<UserConfig>>,
) {
    if let Some(existing) = eq_win_slot.borrow().as_ref() {
        existing.present();
        return;
    }

    let win = Window::builder()
        .title("Equalizer & DSP Studio — Audio Engine V10")
        .transient_for(parent)
        .default_width(750)
        .default_height(490)
        .icon_name("io.github.leumicard_code.AudioEngineV10")
        .build();
    win.add_css_class("studio-window");

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(14);
    root.set_margin_end(14);

    // Top Header + Auto Headroom Toggle
    let header_row = GtkBox::new(Orientation::Horizontal, 10);
    let lbl_header = Label::new(Some("10-Band Graphic Studio Equalizer (-12 dB to +12 dB)"));
    lbl_header.add_css_class("track-title");
    lbl_header.set_halign(Align::Start);
    lbl_header.set_hexpand(true);

    let init_headroom = user_cfg.borrow().auto_headroom;
    let btn_headroom = Button::with_label(if init_headroom {
        "✓ Auto Headroom: ON"
    } else {
        "Auto Headroom: OFF"
    });
    btn_headroom.add_css_class(if init_headroom {
        "preset-btn-active"
    } else {
        "preset-btn"
    });

    {
        let engine = Rc::clone(engine);
        let cfg = Rc::clone(user_cfg);
        btn_headroom.connect_clicked(move |btn| {
            let mut c = cfg.borrow_mut();
            c.auto_headroom = !c.auto_headroom;
            let on = c.auto_headroom;
            c.save();
            drop(c);
            engine.borrow_mut().set_auto_headroom(on);
            btn.set_label(if on {
                "✓ Auto Headroom: ON"
            } else {
                "Auto Headroom: OFF"
            });
            btn.remove_css_class("preset-btn");
            btn.remove_css_class("preset-btn-active");
            btn.add_css_class(if on { "preset-btn-active" } else { "preset-btn" });
        });
    }

    header_row.append(&lbl_header);
    header_row.append(&btn_headroom);
    root.append(&header_row);

    // Studio Presets Frame
    let preset_frame = make_section_frame(" Studio Presets ");
    let preset_row = GtkBox::new(Orientation::Horizontal, 8);
    preset_row.set_margin_top(8);
    preset_row.set_margin_bottom(8);
    preset_row.set_margin_start(10);
    preset_row.set_margin_end(10);
    preset_row.set_homogeneous(true);

    let current_gains = *engine.borrow().get_eq_bands();
    let mut band_scales: Vec<Scale> = Vec::with_capacity(10);
    let mut band_labels: Vec<Label> = Vec::with_capacity(10);
    let updating_sliders = Rc::new(Cell::new(false));

    // 10 Faders Box
    let bands_frame = make_section_frame(" Frequency Response Faders ");
    let bands_row = GtkBox::new(Orientation::Horizontal, 8);
    bands_row.set_margin_top(10);
    bands_row.set_margin_bottom(10);
    bands_row.set_margin_start(12);
    bands_row.set_margin_end(12);
    bands_row.set_homogeneous(true);
    bands_row.set_vexpand(true);

    for (band_idx, &freq) in EQ_FREQUENCIES.iter().enumerate() {
        let col = GtkBox::new(Orientation::Vertical, 6);
        col.set_vexpand(true);

        let init_db = current_gains[band_idx];
        let lbl_val = Label::new(Some(&format!("{:+.0}dB", init_db)));
        lbl_val.add_css_class("eq-db-label");

        let scale = Scale::with_range(Orientation::Vertical, -12.0, 12.0, 0.5);
        scale.set_inverted(true);
        scale.set_draw_value(false);
        scale.set_vexpand(true);
        scale.set_value(init_db);
        scale.add_css_class("studio-scale");

        let freq_text = if freq >= 1000 {
            format!("{:.1}k", freq as f32 / 1000.0)
        } else {
            format!("{}Hz", freq)
        };
        let lbl_freq = Label::new(Some(&freq_text));
        lbl_freq.add_css_class("eq-freq-label");

        {
            let engine = Rc::clone(engine);
            let cfg = Rc::clone(user_cfg);
            let lbl_val = lbl_val.clone();
            let updating = Rc::clone(&updating_sliders);
            scale.connect_value_changed(move |s| {
                let gain_db = s.value();
                lbl_val.set_text(&format!("{:+.0}dB", gain_db));
                if !updating.get() {
                    let mut eng = engine.borrow_mut();
                    eng.set_eq_band(band_idx, gain_db);
                    let mut c = cfg.borrow_mut();
                    c.eq_bands = *eng.get_eq_bands();
                    c.save();
                }
            });
        }

        col.append(&lbl_val);
        col.append(&scale);
        col.append(&lbl_freq);

        bands_row.append(&col);
        band_scales.push(scale);
        band_labels.push(lbl_val);
    }

    for preset in ALL_PRESETS {
        let btn = Button::with_label(&preset.to_string());
        btn.add_css_class("preset-btn");

        let engine = Rc::clone(engine);
        let cfg = Rc::clone(user_cfg);
        let scales = band_scales.clone();
        let labels = band_labels.clone();
        let updating = Rc::clone(&updating_sliders);

        btn.connect_clicked(move |_| {
            let gains = preset.gains();
            engine.borrow_mut().set_preset(preset);
            {
                let mut c = cfg.borrow_mut();
                c.eq_bands = gains;
                c.save();
            }
            updating.set(true);
            for i in 0..10 {
                scales[i].set_value(gains[i]);
                labels[i].set_text(&format!("{:+.0}dB", gains[i]));
            }
            updating.set(false);
        });

        preset_row.append(&btn);
    }

    preset_frame.set_child(Some(&preset_row));
    bands_frame.set_child(Some(&bands_row));

    root.append(&preset_frame);
    root.append(&bands_frame);

    let bottom_row = GtkBox::new(Orientation::Horizontal, 10);
    bottom_row.set_halign(Align::Center);
    let btn_close = Button::with_label("Close Studio");
    btn_close.add_css_class("studio-primary-btn");
    {
        let win_c = win.clone();
        btn_close.connect_clicked(move |_| {
            win_c.close();
        });
    }
    bottom_row.append(&btn_close);
    root.append(&bottom_row);

    {
        let slot = Rc::clone(eq_win_slot);
        let cfg = Rc::clone(user_cfg);
        win.connect_close_request(move |_| {
            cfg.borrow().save();
            *slot.borrow_mut() = None;
            glib::Propagation::Proceed
        });
    }

    win.set_child(Some(&root));
    *eq_win_slot.borrow_mut() = Some(win.clone());
    win.present();
}

// ============================================================================
// Main Application Builder
// ============================================================================

fn build_ui(app: &Application) {
    let loaded_cfg = UserConfig::load();
    let initial_mode = loaded_cfg.theme;
    let initial_vol = loaded_cfg.volume_pct;
    let initial_eq = loaded_cfg.eq_bands;
    let initial_theme = Theme::get(initial_mode);

    let css_provider = CssProvider::new();
    css_provider.load_from_string(&initial_theme.to_gtk_css());
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 100,
        );

        let icon_theme = gtk::IconTheme::for_display(&display);
        for candidate in [
            "/app/share/icons",
            "/home/shay/code1/gtkplayer/data/icons",
            "data/icons",
        ] {
            if Path::new(candidate).exists() {
                icon_theme.add_search_path(candidate);
            }
        }
    }

    let user_cfg = Rc::new(RefCell::new(loaded_cfg.clone()));
    let active_theme = Rc::new(RefCell::new(initial_theme.clone()));
    let spectrum_state = Rc::new(RefCell::new(SpectrumState::new(SPECTRUM_BARS)));
    let is_seeking = Rc::new(Cell::new(false));
    let eq_win_slot: Rc<RefCell<Option<Window>>> = Rc::new(RefCell::new(None));

    let mut raw_engine = AudioEngine::new().expect("Failed to initialize GStreamer AudioEngine");
    raw_engine.set_auto_headroom(loaded_cfg.auto_headroom);
    raw_engine.set_volume(initial_vol as f64 / 100.0);
    raw_engine.set_all_eq_bands(initial_eq);
    let engine = Rc::new(RefCell::new(raw_engine));

    let restored_playlist: Vec<PlaylistItem> = loaded_cfg
        .playlist
        .into_iter()
        .map(|t| PlaylistItem {
            path: t.path,
            title: t.title,
            artist: t.artist,
        })
        .collect();

    let app_state = Rc::new(RefCell::new(AppState {
        playlist: restored_playlist,
        visible_indices: Vec::new(),
        filter_query: String::new(),
        current_idx: loaded_cfg.current_idx,
        is_playing: false,
    }));

    // ── Main Window (Pure GTK 4, Native Titlebar, MenuBar Visible) ─────────
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Audio Engine V10 — Audiophile Studio")
        .default_width(545)
        .default_height(760)
        .icon_name("io.github.leumicard_code.AudioEngineV10")
        .show_menubar(true)
        .build();
    window.add_css_class("studio-window");

    let outer_vbox = GtkBox::new(Orientation::Vertical, 0);
    let main_vbox = GtkBox::new(Orientation::Vertical, 10);
    main_vbox.set_margin_top(10);
    main_vbox.set_margin_bottom(10);
    main_vbox.set_margin_start(12);
    main_vbox.set_margin_end(12);
    main_vbox.set_vexpand(true);

    // ========================================================================
    // 1. Audio Controller Section
    // ========================================================================
    let ctrl_frame = make_section_frame(" Audio Controller ");
    let ctrl_box = GtkBox::new(Orientation::Vertical, 6);
    ctrl_box.set_margin_top(8);
    ctrl_box.set_margin_bottom(10);
    ctrl_box.set_margin_start(12);
    ctrl_box.set_margin_end(12);

    let lbl_title = Label::new(Some("No Track Loaded"));
    lbl_title.add_css_class("track-title");
    lbl_title.set_halign(Align::Start);
    lbl_title.set_xalign(0.0);
    lbl_title.set_ellipsize(pango::EllipsizeMode::End);

    let lbl_artist = Label::new(Some("Add audio files or a music folder to start listening"));
    lbl_artist.add_css_class("track-artist");
    lbl_artist.set_halign(Align::Start);
    lbl_artist.set_xalign(0.0);
    lbl_artist.set_ellipsize(pango::EllipsizeMode::End);

    ctrl_box.append(&lbl_title);
    ctrl_box.append(&lbl_artist);

    // Seek Bar Row
    let seek_row = GtkBox::new(Orientation::Horizontal, 10);
    seek_row.set_margin_top(4);
    let lbl_time = Label::new(Some("0:00 / 0:00"));
    lbl_time.add_css_class("time-label");
    lbl_time.set_width_request(95);
    lbl_time.set_xalign(0.0);

    let seek_scale = Scale::with_range(Orientation::Horizontal, 0.0, 1000.0, 1.0);
    seek_scale.set_draw_value(false);
    seek_scale.set_hexpand(true);
    seek_scale.add_css_class("studio-scale");

    seek_row.append(&lbl_time);
    seek_row.append(&seek_scale);
    ctrl_box.append(&seek_row);

    // Circular Transport & Volume Row
    let trans_row = GtkBox::new(Orientation::Horizontal, 8);
    trans_row.set_margin_top(4);
    trans_row.set_valign(Align::Center);

    let btn_prev = IconButton::new(
        IconName::Prev,
        IconRole::Standard,
        "Previous Track (Ctrl+Left)",
        &initial_theme,
    );
    let btn_play = IconButton::new(
        IconName::Play,
        IconRole::Hero,
        "Play / Pause (Ctrl+P)",
        &initial_theme,
    );
    let btn_next = IconButton::new(
        IconName::Next,
        IconRole::Standard,
        "Next Track (Ctrl+Right)",
        &initial_theme,
    );
    let btn_stop = IconButton::new(
        IconName::Stop,
        IconRole::Standard,
        "Stop Playback (Ctrl+S)",
        &initial_theme,
    );
    let btn_eq = IconButton::new(
        IconName::Eq,
        IconRole::Accent,
        "Equalizer & DSP Studio (Ctrl+E)",
        &initial_theme,
    );

    btn_stop.widget().set_margin_end(8);

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let ico_vol = StaticIcon::new(IconName::Volume, 20, &initial_theme);
    let lbl_vol = Label::new(Some(&format!("{}%", initial_vol)));
    lbl_vol.add_css_class("vol-label");
    lbl_vol.set_width_request(40);
    lbl_vol.set_xalign(0.0);

    let volume_scale = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    volume_scale.set_draw_value(false);
    volume_scale.set_value(initial_vol as f64);
    volume_scale.set_width_request(135);
    volume_scale.add_css_class("studio-scale");

    trans_row.append(btn_prev.widget());
    trans_row.append(btn_play.widget());
    trans_row.append(btn_next.widget());
    trans_row.append(btn_stop.widget());
    trans_row.append(btn_eq.widget());
    trans_row.append(&spacer);
    trans_row.append(ico_vol.widget());
    trans_row.append(&lbl_vol);
    trans_row.append(&volume_scale);

    ctrl_box.append(&trans_row);
    ctrl_frame.set_child(Some(&ctrl_box));
    main_vbox.append(&ctrl_frame);

    // ========================================================================
    // 2. Live 64-Band Spectrum Visualizer Section
    // ========================================================================
    let spec_frame = make_section_frame(" Live Spectrum Visualizer ");
    let spec_box = GtkBox::new(Orientation::Vertical, 6);
    spec_box.set_margin_top(8);
    spec_box.set_margin_bottom(8);
    spec_box.set_margin_start(10);
    spec_box.set_margin_end(10);

    let spec_canvas = DrawingArea::builder()
        .content_height(100)
        .hexpand(true)
        .build();

    {
        let spec_state = Rc::clone(&spectrum_state);
        let theme_ref = Rc::clone(&active_theme);
        spec_canvas.set_draw_func(move |_, cr, w, h| {
            let th = theme_ref.borrow();
            let snap = spec_state.borrow();
            let wf = w as f64;
            let hf = h as f64;

            let (cr_r, cr_g, cr_b) = th.card_bg.cairo_f64();
            cr.set_source_rgb(cr_r, cr_g, cr_b);
            cairo_rounded_rect(cr, 0.0, 0.0, wf, hf, 6.0);
            let _ = cr.fill();

            let count = snap.bars.len().max(1);
            let pad_x = 10.0;
            let pad_y = 8.0;
            let inner_w = (wf - pad_x * 2.0).max(count as f64);
            let inner_h = (hf - pad_y * 2.0).max(10.0);
            let gap = 2.0;
            let step = inner_w / count as f64;

            let (sr, sg, sb) = th.spec_slot.cairo_f64();
            let (tr, tg, tb) = th.text_main.cairo_f64();

            for i in 0..count {
                let x0 = (pad_x + i as f64 * step).round();
                let x1 = (pad_x + (i + 1) as f64 * step).round();
                let bar_w = (x1 - x0 - gap).max(2.0);

                cr.set_source_rgb(sr, sg, sb);
                cairo_rounded_rect(cr, x0, pad_y, bar_w, inner_h, 2.0);
                let _ = cr.fill();

                let val = snap.bars[i].clamp(0.0, 1.0) as f64;
                let bar_h = (val * inner_h).round().max(3.0);
                let y = pad_y + inner_h - bar_h;

                let (br, bg, bb) = th.bar_color(i, count).cairo_f64();
                cr.set_source_rgb(br, bg, bb);
                cairo_rounded_rect(cr, x0, y, bar_w, bar_h, 2.0);
                let _ = cr.fill();

                let peak = snap.peaks[i].clamp(0.0, 1.0) as f64;
                if peak > 0.06 {
                    let peak_y = (pad_y + inner_h - (peak * inner_h).round() - 2.0)
                        .clamp(pad_y, pad_y + inner_h - 2.0);
                    cr.set_source_rgb(tr, tg, tb);
                    cr.rectangle(x0, peak_y, bar_w, 2.0);
                    let _ = cr.fill();
                }
            }
        });
    }

    let lbl_spec_mode = Label::new(Some(
        "64-Band Logarithmic RTA • Peak Hold • Smart Anti-Clipping Headroom",
    ));
    lbl_spec_mode.add_css_class("spec-caption");
    lbl_spec_mode.set_halign(Align::Start);

    spec_box.append(&spec_canvas);
    spec_box.append(&lbl_spec_mode);
    spec_frame.set_child(Some(&spec_box));
    main_vbox.append(&spec_frame);

    // ========================================================================
    // 3. Playlist & Media Library Section
    // ========================================================================
    let pl_frame = make_section_frame(" Playlist & Media Library ");
    pl_frame.set_vexpand(true);
    let pl_box = GtkBox::new(Orientation::Vertical, 8);
    pl_box.set_margin_top(8);
    pl_box.set_margin_bottom(10);
    pl_box.set_margin_start(10);
    pl_box.set_margin_end(10);
    pl_box.set_vexpand(true);

    let filter_row = GtkBox::new(Orientation::Horizontal, 6);
    filter_row.set_valign(Align::Center);

    let ico_search = StaticIcon::new(IconName::Search, 18, &initial_theme);
    let lbl_filter = Label::new(Some("Filter:"));
    lbl_filter.add_css_class("filter-label");

    let txt_filter = Entry::new();
    txt_filter.add_css_class("studio-filter");
    txt_filter.set_hexpand(true);
    txt_filter.set_margin_end(4);

    let btn_add_files = IconButton::new(
        IconName::AddFile,
        IconRole::Compact,
        "Add Audio Files (Ctrl+O)",
        &initial_theme,
    );
    let btn_add_folder = IconButton::new(
        IconName::Folder,
        IconRole::Compact,
        "Add Music Folder",
        &initial_theme,
    );
    let btn_clear = IconButton::new(
        IconName::Trash,
        IconRole::Compact,
        "Clear Playlist (Ctrl+W)",
        &initial_theme,
    );

    filter_row.append(ico_search.widget());
    filter_row.append(&lbl_filter);
    filter_row.append(&txt_filter);
    filter_row.append(btn_add_files.widget());
    filter_row.append(btn_add_folder.widget());
    filter_row.append(btn_clear.widget());

    let scrolled = ScrolledWindow::new();
    scrolled.add_css_class("playlist-scroll");
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);

    let list_box = ListBox::new();
    list_box.add_css_class("studio-playlist");
    list_box.set_selection_mode(SelectionMode::Single);
    list_box.set_activate_on_single_click(false);
    scrolled.set_child(Some(&list_box));

    pl_box.append(&filter_row);
    pl_box.append(&scrolled);
    pl_frame.set_child(Some(&pl_box));
    main_vbox.append(&pl_frame);

    // ========================================================================
    // 4. Bottom 3-Column Status Bar
    // ========================================================================
    let status_bar = GtkBox::new(Orientation::Horizontal, 12);
    status_bar.add_css_class("status-bar");

    let status_left = Label::new(Some("System Ready"));
    status_left.add_css_class("status-label");
    status_left.set_halign(Align::Start);
    status_left.set_xalign(0.0);
    status_left.set_hexpand(true);
    status_left.set_ellipsize(pango::EllipsizeMode::End);

    let status_mid = Label::new(Some("0 tracks"));
    status_mid.add_css_class("status-label");
    status_mid.set_halign(Align::Center);
    status_mid.set_width_request(110);

    let status_right = Label::new(Some(&format!("{} • DSP Active", initial_theme.name)));
    status_right.add_css_class("status-label");
    status_right.set_halign(Align::End);

    status_bar.append(&status_left);
    status_bar.append(&status_mid);
    status_bar.append(&status_right);

    outer_vbox.append(&main_vbox);
    outer_vbox.append(&status_bar);
    window.set_child(Some(&outer_vbox));

    // Populate initial playlist
    {
        let mut st = app_state.borrow_mut();
        rebuild_playlist_listbox(&mut st, &list_box);
        status_mid.set_text(&format!("{} tracks", st.playlist.len()));
        if let Some(idx) = st.current_idx {
            if let Some(item) = st.playlist.get(idx) {
                lbl_title.set_text(&item.title);
                if !item.artist.is_empty() {
                    lbl_artist.set_text(&item.artist);
                } else {
                    lbl_artist.set_text("Ready to play saved track");
                }
                status_left.set_text(&format!("Ready: {}", item.title));
            }
        }
    }

    // ========================================================================
    // Core Playback & Playlist Closures
    // ========================================================================
    let play_track: Rc<dyn Fn(usize)> = {
        let engine = Rc::clone(&engine);
        let state = Rc::clone(&app_state);
        let cfg = Rc::clone(&user_cfg);
        let lbl_title = lbl_title.clone();
        let lbl_artist = lbl_artist.clone();
        let btn_play = btn_play.clone();
        let list_box = list_box.clone();
        let status_left = status_left.clone();

        Rc::new(move |idx: usize| {
            let (path_str, title, artist) = {
                let mut st = state.borrow_mut();
                if idx >= st.playlist.len() {
                    return;
                }
                st.current_idx = Some(idx);
                st.is_playing = true;
                let item = st.playlist[idx].clone();
                rebuild_playlist_listbox(&mut st, &list_box);
                sync_playlist_to_config(&st, &cfg);
                (
                    item.path.to_string_lossy().to_string(),
                    item.title,
                    item.artist,
                )
            };

            {
                let eng = engine.borrow();
                let _ = eng.load_file(&path_str);
                let _ = eng.play();
            }

            lbl_title.set_text(&title);
            if artist.is_empty() {
                lbl_artist.set_text("Playing Local Studio Stream");
            } else {
                lbl_artist.set_text(&artist);
            }
            btn_play.set_icon(IconName::Pause);
            status_left.set_text(&format!("Playing: {}", title));
        })
    };

    let add_paths_action: Rc<dyn Fn(Vec<PathBuf>) -> Option<usize>> = {
        let state = Rc::clone(&app_state);
        let cfg = Rc::clone(&user_cfg);
        let list_box = list_box.clone();
        let status_mid = status_mid.clone();

        Rc::new(move |paths: Vec<PathBuf>| {
            let mut first_added = None;
            let mut st = state.borrow_mut();
            for path in paths {
                if !is_audio_file(&path) {
                    continue;
                }
                if first_added.is_none() {
                    first_added = Some(st.playlist.len());
                }
                let (title, artist) = parse_stem_title_artist(&path);
                st.playlist.push(PlaylistItem {
                    path,
                    title,
                    artist,
                });
            }
            rebuild_playlist_listbox(&mut st, &list_box);
            status_mid.set_text(&format!("{} tracks", st.playlist.len()));
            sync_playlist_to_config(&st, &cfg);
            first_added
        })
    };

    // Handle CLI arguments
    let cli_paths = collect_cli_audio_paths();
    if !cli_paths.is_empty() {
        if let Some(start_idx) = add_paths_action(cli_paths) {
            play_track(start_idx);
        }
    }

    let pick_files_action: Rc<dyn Fn()> = {
        let win = window.clone();
        let add_paths = Rc::clone(&add_paths_action);
        Rc::new(move || {
            let filter = FileFilter::new();
            filter.set_name(Some("Audio Files"));
            for &ext in AUDIO_EXTENSIONS {
                filter.add_suffix(ext);
            }
            let filters = gio::ListStore::new::<FileFilter>();
            filters.append(&filter);

            let dlg = FileDialog::builder()
                .title("Add Audio Files")
                .filters(&filters)
                .build();

            let add_paths = Rc::clone(&add_paths);
            dlg.open_multiple(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(files) = res {
                    let mut out = Vec::new();
                    for i in 0..files.n_items() {
                        if let Some(obj) = files.item(i) {
                            if let Ok(gfile) = obj.downcast::<gio::File>() {
                                if let Some(p) = gfile.path() {
                                    out.push(p);
                                }
                            }
                        }
                    }
                    if !out.is_empty() {
                        add_paths(out);
                    }
                }
            });
        })
    };

    let pick_folder_action: Rc<dyn Fn()> = {
        let win = window.clone();
        let add_paths = Rc::clone(&add_paths_action);
        Rc::new(move || {
            let dlg = FileDialog::builder().title("Add Music Folder").build();
            let add_paths = Rc::clone(&add_paths);
            dlg.select_folder(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(gfile) = res {
                    if let Some(folder) = gfile.path() {
                        let mut files: Vec<PathBuf> = WalkDir::new(folder)
                            .into_iter()
                            .filter_map(Result::ok)
                            .filter(|e| e.file_type().is_file() && is_audio_file(e.path()))
                            .map(|e| e.into_path())
                            .collect();
                        files.sort();
                        if !files.is_empty() {
                            add_paths(files);
                        }
                    }
                }
            });
        })
    };

    let clear_playlist_action: Rc<dyn Fn()> = {
        let state = Rc::clone(&app_state);
        let cfg = Rc::clone(&user_cfg);
        let engine = Rc::clone(&engine);
        let list_box = list_box.clone();
        let status_left = status_left.clone();
        let status_mid = status_mid.clone();
        let lbl_title = lbl_title.clone();
        let lbl_artist = lbl_artist.clone();
        let lbl_time = lbl_time.clone();
        let seek_scale = seek_scale.clone();
        let btn_play = btn_play.clone();

        Rc::new(move || {
            {
                let mut st = state.borrow_mut();
                st.playlist.clear();
                st.visible_indices.clear();
                st.current_idx = None;
                st.is_playing = false;
                while let Some(child) = list_box.first_child() {
                    list_box.remove(&child);
                }
                status_left.set_text("Playlist Cleared");
                status_mid.set_text("0 tracks");
                lbl_title.set_text("No Track Loaded");
                lbl_artist.set_text("Add audio files or a music folder to start listening");
                lbl_time.set_text("0:00 / 0:00");
                seek_scale.set_value(0.0);
                btn_play.set_icon(IconName::Play);
                sync_playlist_to_config(&st, &cfg);
            }
            let _ = engine.borrow().stop();
        })
    };

    let toggle_play_action: Rc<dyn Fn()> = {
        let engine = Rc::clone(&engine);
        let state = Rc::clone(&app_state);
        let btn_play = btn_play.clone();
        let play_selected = Rc::clone(&play_track);

        Rc::new(move || {
            let mut st = state.borrow_mut();
            if let Some(cur_idx) = st.current_idx {
                if st.is_playing {
                    let _ = engine.borrow().pause();
                    st.is_playing = false;
                    btn_play.set_icon(IconName::Play);
                } else {
                    let resumed = {
                        let eng = engine.borrow();
                        if eng.get_duration().is_some() {
                            let _ = eng.play();
                            true
                        } else {
                            false
                        }
                    };
                    if resumed {
                        st.is_playing = true;
                        btn_play.set_icon(IconName::Pause);
                    } else {
                        drop(st);
                        play_selected(cur_idx);
                    }
                }
            } else if !st.playlist.is_empty() {
                drop(st);
                play_selected(0);
            }
        })
    };

    let stop_action: Rc<dyn Fn()> = {
        let engine = Rc::clone(&engine);
        let state = Rc::clone(&app_state);
        let btn_play = btn_play.clone();
        let seek_scale = seek_scale.clone();
        let lbl_time = lbl_time.clone();
        Rc::new(move || {
            state.borrow_mut().is_playing = false;
            btn_play.set_icon(IconName::Play);
            seek_scale.set_value(0.0);
            lbl_time.set_text("0:00 / 0:00");
            let _ = engine.borrow().stop();
        })
    };

    let next_action: Rc<dyn Fn()> = {
        let state = Rc::clone(&app_state);
        let play = Rc::clone(&play_track);
        Rc::new(move || {
            let next_idx = {
                let st = state.borrow();
                if st.playlist.is_empty() {
                    None
                } else {
                    Some(st.current_idx.map(|i| (i + 1) % st.playlist.len()).unwrap_or(0))
                }
            };
            if let Some(n) = next_idx {
                play(n);
            }
        })
    };

    let prev_action: Rc<dyn Fn()> = {
        let state = Rc::clone(&app_state);
        let play = Rc::clone(&play_track);
        Rc::new(move || {
            let prev_idx = {
                let st = state.borrow();
                if st.playlist.is_empty() {
                    None
                } else {
                    Some(
                        st.current_idx
                            .map(|i| if i > 0 { i - 1 } else { st.playlist.len() - 1 })
                            .unwrap_or(0),
                    )
                }
            };
            if let Some(p) = prev_idx {
                play(p);
            }
        })
    };

    let apply_theme_action: Rc<dyn Fn(ThemeMode)> = {
        let active_theme = Rc::clone(&active_theme);
        let cfg = Rc::clone(&user_cfg);
        let css_provider = css_provider.clone();
        let btn_prev = btn_prev.clone();
        let btn_play = btn_play.clone();
        let btn_next = btn_next.clone();
        let btn_stop = btn_stop.clone();
        let btn_eq = btn_eq.clone();
        let ico_vol = ico_vol.clone();
        let ico_search = ico_search.clone();
        let btn_add_files = btn_add_files.clone();
        let btn_add_folder = btn_add_folder.clone();
        let btn_clear = btn_clear.clone();
        let spec_canvas = spec_canvas.clone();
        let status_right = status_right.clone();

        Rc::new(move |mode: ThemeMode| {
            let new_theme = Theme::get(mode);
            css_provider.load_from_string(&new_theme.to_gtk_css());
            *active_theme.borrow_mut() = new_theme.clone();
            {
                let mut c = cfg.borrow_mut();
                c.theme = mode;
                c.save();
            }
            btn_prev.set_theme(&new_theme);
            btn_play.set_theme(&new_theme);
            btn_next.set_theme(&new_theme);
            btn_stop.set_theme(&new_theme);
            btn_eq.set_theme(&new_theme);
            ico_vol.set_theme(&new_theme);
            ico_search.set_theme(&new_theme);
            btn_add_files.set_theme(&new_theme);
            btn_add_folder.set_theme(&new_theme);
            btn_clear.set_theme(&new_theme);
            status_right.set_text(&format!("{} • DSP Active", new_theme.name));
            spec_canvas.queue_draw();
        })
    };

    // ========================================================================
    // Wire Controls, Sliders, Search & Playlist Activation
    // ========================================================================
    {
        let toggle = Rc::clone(&toggle_play_action);
        btn_play.on_click(move || toggle());
    }
    {
        let stop = Rc::clone(&stop_action);
        btn_stop.on_click(move || stop());
    }
    {
        let next = Rc::clone(&next_action);
        btn_next.on_click(move || next());
    }
    {
        let prev = Rc::clone(&prev_action);
        btn_prev.on_click(move || prev());
    }
    {
        let win = window.clone();
        let eq_slot = Rc::clone(&eq_win_slot);
        let engine = Rc::clone(&engine);
        let cfg = Rc::clone(&user_cfg);
        btn_eq.on_click(move || {
            open_equalizer_window(&win, &eq_slot, &engine, &cfg);
        });
    }
    {
        let pick_files = Rc::clone(&pick_files_action);
        btn_add_files.on_click(move || pick_files());
    }
    {
        let pick_folder = Rc::clone(&pick_folder_action);
        btn_add_folder.on_click(move || pick_folder());
    }
    {
        let clear = Rc::clone(&clear_playlist_action);
        btn_clear.on_click(move || clear());
    }

    // Live Filter Entry
    {
        let state = Rc::clone(&app_state);
        let list_box = list_box.clone();
        txt_filter.connect_changed(move |entry| {
            let mut st = state.borrow_mut();
            st.filter_query = entry.text().to_string();
            rebuild_playlist_listbox(&mut st, &list_box);
        });
    }

    // Double-Click or Enter on Playlist Row
    {
        let state = Rc::clone(&app_state);
        let play = Rc::clone(&play_track);
        list_box.connect_row_activated(move |_, row| {
            let pos = row.index();
            if pos >= 0 {
                let real_idx = state.borrow().visible_indices.get(pos as usize).copied();
                if let Some(idx) = real_idx {
                    play(idx);
                }
            }
        });
    }

    // Volume Slider
    {
        let engine = Rc::clone(&engine);
        let cfg = Rc::clone(&user_cfg);
        let lbl_vol = lbl_vol.clone();
        volume_scale.connect_value_changed(move |s| {
            let pct = s.value().round() as i32;
            lbl_vol.set_text(&format!("{}%", pct));
            engine.borrow().set_volume(pct as f64 / 100.0);
            let mut c = cfg.borrow_mut();
            c.volume_pct = pct;
            c.save();
        });
    }

    // Seek Slider
    {
        let gesture = GestureClick::new();
        {
            let seeking = Rc::clone(&is_seeking);
            gesture.connect_pressed(move |_, _, _, _| {
                seeking.set(true);
            });
        }
        {
            let seeking = Rc::clone(&is_seeking);
            gesture.connect_released(move |_, _, _, _| {
                seeking.set(false);
            });
        }
        seek_scale.add_controller(gesture);

        let engine = Rc::clone(&engine);
        let seeking = Rc::clone(&is_seeking);
        let lbl_time = lbl_time.clone();
        seek_scale.connect_change_value(move |_, _, val| {
            seeking.set(true);
            let eng = engine.borrow();
            if let Some(dur) = eng.get_duration() {
                let clamped = val.clamp(0.0, 1000.0);
                let target_sec = (clamped / 1000.0) * dur;
                lbl_time.set_text(&format!(
                    "{} / {}",
                    format_time(target_sec as u64),
                    format_time(dur as u64)
                ));
                let _ = eng.seek_position(target_sec);
            }
            seeking.set(false);
            glib::Propagation::Proceed
        });
    }

    // ========================================================================
    // MenuBar Actions & Keyboard Shortcuts
    // ========================================================================
    let menubar = gio::Menu::new();

    let file_menu = gio::Menu::new();
    file_menu.append(Some("Add Audio Files..."), Some("app.add_files"));
    file_menu.append(Some("Add Folder..."), Some("app.add_folder"));
    file_menu.append(Some("Clear Playlist"), Some("app.clear_playlist"));
    file_menu.append(Some("Exit"), Some("app.quit"));
    menubar.append_submenu(Some("File"), &file_menu);

    let play_menu = gio::Menu::new();
    play_menu.append(Some("Play / Pause"), Some("app.play_toggle"));
    play_menu.append(Some("Next Track"), Some("app.next_track"));
    play_menu.append(Some("Previous Track"), Some("app.prev_track"));
    play_menu.append(Some("Stop"), Some("app.stop_track"));
    menubar.append_submenu(Some("Playback"), &play_menu);

    let view_menu = gio::Menu::new();
    view_menu.append(Some("Equalizer & DSP Studio..."), Some("app.open_eq"));

    let skin_menu = gio::Menu::new();
    for mode in ALL_THEMES {
        let th = Theme::get(mode);
        let act_name = match mode {
            ThemeMode::FrostLight => "app.skin_frost",
            ThemeMode::DarkElegance => "app.skin_elegance",
            ThemeMode::CyberpunkNeon => "app.skin_cyberpunk",
            ThemeMode::WarmVintage => "app.skin_vintage",
            ThemeMode::MatrixEmerald => "app.skin_emerald",
            ThemeMode::TokyoNight => "app.skin_tokyo",
        };
        skin_menu.append(Some(th.name), Some(act_name));
    }
    view_menu.append_submenu(Some("Theme & Skins"), &skin_menu);
    menubar.append_submenu(Some("View"), &view_menu);

    let help_menu = gio::Menu::new();
    help_menu.append(Some("About Audio Engine V10"), Some("app.about"));
    menubar.append_submenu(Some("Help"), &help_menu);

    app.set_menubar(Some(&menubar));

    let register_action = |name: &str, accel: Option<&str>, cb: Box<dyn Fn() + 'static>| {
        let act = gio::SimpleAction::new(name, None);
        act.connect_activate(move |_, _| cb());
        app.add_action(&act);
        if let Some(acc) = accel {
            app.set_accels_for_action(&format!("app.{}", name), &[acc]);
        }
    };

    {
        let pick_files = Rc::clone(&pick_files_action);
        register_action("add_files", Some("<Primary>o"), Box::new(move || pick_files()));
    }
    {
        let pick_folder = Rc::clone(&pick_folder_action);
        register_action("add_folder", None, Box::new(move || pick_folder()));
    }
    {
        let clear = Rc::clone(&clear_playlist_action);
        register_action("clear_playlist", Some("<Primary>w"), Box::new(move || clear()));
    }
    {
        let win = window.clone();
        register_action("quit", Some("<Primary>q"), Box::new(move || win.close()));
    }
    {
        let toggle = Rc::clone(&toggle_play_action);
        register_action("play_toggle", Some("<Primary>p"), Box::new(move || toggle()));
    }
    {
        let next = Rc::clone(&next_action);
        register_action("next_track", Some("<Primary>Right"), Box::new(move || next()));
    }
    {
        let prev = Rc::clone(&prev_action);
        register_action("prev_track", Some("<Primary>Left"), Box::new(move || prev()));
    }
    {
        let stop = Rc::clone(&stop_action);
        register_action("stop_track", Some("<Primary>s"), Box::new(move || stop()));
    }
    {
        let win = window.clone();
        let eq_slot = Rc::clone(&eq_win_slot);
        let engine = Rc::clone(&engine);
        let cfg = Rc::clone(&user_cfg);
        register_action(
            "open_eq",
            Some("<Primary>e"),
            Box::new(move || {
                open_equalizer_window(&win, &eq_slot, &engine, &cfg);
            }),
        );
    }

    for (act_id, mode) in [
        ("skin_frost", ThemeMode::FrostLight),
        ("skin_elegance", ThemeMode::DarkElegance),
        ("skin_cyberpunk", ThemeMode::CyberpunkNeon),
        ("skin_vintage", ThemeMode::WarmVintage),
        ("skin_emerald", ThemeMode::MatrixEmerald),
        ("skin_tokyo", ThemeMode::TokyoNight),
    ] {
        let apply = Rc::clone(&apply_theme_action);
        register_action(act_id, None, Box::new(move || apply(mode)));
    }

    {
        let win = window.clone();
        register_action(
            "about",
            Some("F1"),
            Box::new(move || {
                let dlg = gtk::AboutDialog::builder()
                    .transient_for(&win)
                    .modal(true)
                    .program_name("Audio Engine V10 — Audiophile DSP Studio")
                    .version("10.0.0 (Pure GTK 4 Edition)")
                    .authors(vec!["Shay Kadosh (System Architect & Lead Developer)"])
                    .copyright("© 2000–2026 Shay Kadosh. All Rights Reserved.")
                    .comments(
                        "• 10-Band GStreamer Graphic Equalizer\n\
                         • 64-Band Logarithmic RTA Spectrum + Peak Hold\n\
                         • Smart Anti-Clipping Auto-Headroom\n\
                         • Full Playlist, EQ & Skin Persistence\n\
                         • Pure GTK 4 + Cairo Hardware-Accelerated Studio UI",
                    )
                    .build();
                dlg.present();
            }),
        );
    }

    // ========================================================================
    // High-Precision UI Timer (~30 FPS Spectrum + Metadata + Seek Sync)
    // ========================================================================
    let engine_timer = Rc::clone(&engine);
    let state_timer = Rc::clone(&app_state);
    let cfg_timer = Rc::clone(&user_cfg);
    let spectrum_timer = Rc::clone(&spectrum_state);
    let seeking_timer = Rc::clone(&is_seeking);
    let spec_canvas_timer = spec_canvas.clone();
    let seek_scale_timer = seek_scale.clone();
    let lbl_time_timer = lbl_time.clone();
    let lbl_title_timer = lbl_title.clone();
    let lbl_artist_timer = lbl_artist.clone();
    let btn_play_timer = btn_play.clone();
    let list_box_timer = list_box.clone();
    let status_left_timer = status_left.clone();
    let play_next_eos = Rc::clone(&play_track);
    let last_sec_shown: Rc<Cell<Option<(u64, u64)>>> = Rc::new(Cell::new(None));

    glib::timeout_add_local(Duration::from_millis(33), move || {
        let mut next_to_play: Option<usize> = None;
        let mut reached_end = false;
        let mut got_fft = false;

        let events = engine_timer.borrow().poll_events();
        for evt in events {
            match evt {
                EngineEvent::Spectrum(fft_bins) => {
                    spectrum_timer.borrow_mut().update_from_fft(&fft_bins);
                    got_fft = true;
                    spec_canvas_timer.queue_draw();
                }
                EngineEvent::MetadataUpdated(meta) => {
                    let mut st = state_timer.borrow_mut();
                    if let Some(cur) = st.current_idx {
                        if let Some(title) = meta.title {
                            if !title.trim().is_empty() {
                                st.playlist[cur].title = title.clone();
                                lbl_title_timer.set_text(&title);
                                status_left_timer.set_text(&format!("Playing: {}", title));
                            }
                        }
                        if let Some(artist) = meta.artist {
                            if !artist.trim().is_empty() {
                                st.playlist[cur].artist = artist.clone();
                                lbl_artist_timer.set_text(&artist);
                            }
                        }
                        rebuild_playlist_listbox(&mut st, &list_box_timer);
                        sync_playlist_to_config(&st, &cfg_timer);
                    }
                }
                EngineEvent::EndOfStream => {
                    let mut st = state_timer.borrow_mut();
                    next_to_play = st
                        .current_idx
                        .and_then(|i| (i + 1 < st.playlist.len()).then_some(i + 1));
                    if next_to_play.is_none() {
                        st.is_playing = false;
                        reached_end = true;
                    }
                }
                EngineEvent::Error(err_msg) => {
                    status_left_timer.set_text(&format!("Audio Error: {}", err_msg));
                }
            }
        }

        if reached_end {
            btn_play_timer.set_icon(IconName::Play);
        }

        if !got_fft {
            let is_playing = state_timer.borrow().is_playing;
            if !is_playing && spectrum_timer.borrow_mut().decay() {
                spec_canvas_timer.queue_draw();
            }
        }

        if !seeking_timer.get() {
            let eng = engine_timer.borrow();
            if let (Some(pos), Some(dur)) = (eng.get_position(), eng.get_duration()) {
                if dur > 0.0 {
                    let pct = ((pos / dur) * 1000.0).clamp(0.0, 1000.0);
                    if (seek_scale_timer.value() - pct).abs() >= 2.0 {
                        seek_scale_timer.set_value(pct);
                    }
                    let pair = (pos as u64, dur as u64);
                    if last_sec_shown.get() != Some(pair) {
                        last_sec_shown.set(Some(pair));
                        lbl_time_timer.set_text(&format!(
                            "{} / {}",
                            format_time(pair.0),
                            format_time(pair.1)
                        ));
                    }
                }
            }
        }

        if let Some(n) = next_to_play {
            play_next_eos(n);
        }

        glib::ControlFlow::Continue
    });

    {
        let cfg = Rc::clone(&user_cfg);
        window.connect_close_request(move |_| {
            cfg.borrow().save();
            glib::Propagation::Proceed
        });
    }

    window.present();
}

fn main() {
    let app = Application::builder()
        .application_id("io.github.shaykadosh.gtkplayer")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(build_ui);
    let empty_args: [&str; 0] = [];
    app.run_with_args(&empty_args);
}
