// ============================================================================
// audio.rs — GStreamer 32-Bit Float (`F32LE`) 10-Band DSP, Spatial & Dynamics
// ============================================================================

use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// Official center frequencies (in Hz) of GStreamer's `equalizer-10bands` plugin.
pub const EQ_FREQUENCIES: [u32; 10] = [29, 59, 119, 237, 474, 947, 1889, 3770, 7523, 15011];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Preset {
    Flat,
    WarmVintage,
    ModernClean,
    BassBoosted,
    VocalEnhance,
    ClubMaster,
}

pub const ALL_PRESETS: [Preset; 6] = [
    Preset::Flat,
    Preset::WarmVintage,
    Preset::ModernClean,
    Preset::BassBoosted,
    Preset::VocalEnhance,
    Preset::ClubMaster,
];

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Preset::Flat => write!(f, "Flat (Normal)"),
            Preset::WarmVintage => write!(f, "Warm Vintage"),
            Preset::ModernClean => write!(f, "Modern Clean"),
            Preset::BassBoosted => write!(f, "Bass Boosted"),
            Preset::VocalEnhance => write!(f, "Vocal Enhancer"),
            Preset::ClubMaster => write!(f, "Club Master"),
        }
    }
}

impl Preset {
    /// Returns the exact 10-band EQ curve (in dB) for each studio preset.
    pub fn gains(&self) -> [f64; 10] {
        match self {
            Preset::Flat => [0.0; 10],
            Preset::WarmVintage => [2.0, 2.5, 1.5, 0.5, 0.0, 0.0, 0.0, -0.5, -1.0, -1.5],
            Preset::ModernClean => [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.5, 1.0],
            Preset::BassBoosted => [6.8, 5.2, 2.8, 1.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0],
            Preset::VocalEnhance => [0.0, 0.0, 0.0, -1.0, 1.0, 2.5, 2.0, 1.5, 0.0, 0.0],
            Preset::ClubMaster => [7.65, 5.85, 3.15, 2.0, 1.0, 0.0, 0.0, 2.0, 2.5, 0.0],
        }
    }
}

/// Metadata automatically extracted from the currently playing audio stream.
#[derive(Debug, Clone, Default)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

/// Real-time events emitted by the DSP engine to the GUI loop.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// Normalized FFT magnitude bins in `0.0..=1.0`.
    Spectrum(Vec<f32>),
    /// Current track reached end-of-stream so the player can advance.
    EndOfStream,
    /// Decoded ID3 / Vorbis / FLAC tags from the active stream.
    MetadataUpdated(TrackMetadata),
    /// Decoding or playback error details.
    Error(String),
}

/// Builds a pure 32-bit float Mid/Side widening matrix for `audiomixmatrix`.
///
/// All coefficients remain strictly within `[-1.0, 1.0]` as required by
/// GStreamer's `audiomixmatrix` parameter validation:
/// - `amount = 0.0`: exact identity `<<1.0, 0.0>, <0.0, 1.0>>` (bit-perfect bypass)
/// - `amount = 1.0`: `<<1.0, -0.35>, <-0.35, 1.0>>` (~2.08x Side/Mid expansion)
fn build_stereo_matrix(amount: f32) -> gst::Array {
    let clamped = amount.clamp(0.0, 1.0) as f64;
    let a = 1.0_f64;
    let b = -0.35_f64 * clamped;
    let row0 = gst::Array::new([a, b]);
    let row1 = gst::Array::new([b, a]);
    gst::Array::new([row0, row1])
}

pub struct AudioEngine {
    pipeline: gst::Pipeline,
    src: gst::Element,
    pre_gain: gst::Element,
    eq: gst::Element,
    compressor: Option<gst::Element>,
    stereo_widener: Option<gst::Element>,
    room_reverb: Option<gst::Element>,
    balance: gst::Element,
    volume: gst::Element,
    eq_bands: [f64; 10],
    auto_headroom: bool,
    punch_enabled: bool,
    stereo_width: f32,
    room_reverb_amount: f32,
    has_limiter: bool,
    spectrum_threshold_db: f32,
    current_metadata: Mutex<TrackMetadata>,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        gst::init().context("Failed to initialize GStreamer")?;

        let pipeline = gst::Pipeline::new();

        // 1. File source & universal stream decoder
        let src = gst::ElementFactory::make("filesrc")
            .name("filesrc0")
            .build()
            .context("Missing 'filesrc' plugin")?;

        let decodebin = gst::ElementFactory::make("decodebin")
            .name("decoder")
            .build()
            .context("Missing 'decodebin' plugin")?;

        // 2. Pure 32-bit float (`F32LE`) DSP entry stage
        let convert = gst::ElementFactory::make("audioconvert")
            .name("convert")
            .build()
            .context("Missing 'audioconvert' plugin")?;

        // Optional pure-F32LE Mid/Side Stereo Stage (`audiomixmatrix`, bypass by default)
        let stereo_widener = gst::ElementFactory::make("audiomixmatrix")
            .name("stereo_widener")
            .property("in-channels", 2u32)
            .property("out-channels", 2u32)
            .property("channel-mask", u64::MAX)
            .build()
            .ok();
        if let Some(ref sw) = stereo_widener {
            if sw.has_property("mode", None) {
                sw.set_property_from_str("mode", "manual");
            }
            sw.set_property("matrix", build_stereo_matrix(0.0));
        }

        // Enforce 2-channel `F32LE` when `audiomixmatrix` is present so mono tracks
        // are cleanly upmixed before the 2x2 matrix without ever leaving 32-bit float.
        let mut caps_builder = gst::Caps::builder("audio/x-raw").field("format", "F32LE");
        if stereo_widener.is_some() {
            caps_builder = caps_builder.field("channels", 2i32);
        }
        let f32_caps = caps_builder.build();

        let dsp_caps = gst::ElementFactory::make("capsfilter")
            .name("dsp_f32_caps")
            .property("caps", &f32_caps)
            .build()
            .context("Missing 'capsfilter' plugin")?;

        // High-precision Sinc resampler (quality 10 = max)
        let resample = gst::ElementFactory::make("audioresample")
            .name("resample")
            .property("quality", 10i32)
            .build()
            .context("Missing 'audioresample' plugin")?;

        // 3. Smart Pre-Gain (automatic anti-clipping headroom + optional soft-knee drive)
        let pre_gain = gst::ElementFactory::make("volume")
            .name("pre_gain")
            .property("volume", 1.0f64)
            .build()
            .context("Missing 'volume' plugin")?;

        // 4. 10-Band Graphic Studio Equalizer (native F32LE)
        let eq = gst::ElementFactory::make("equalizer-10bands")
            .name("eq")
            .build()
            .context("Missing 'equalizer-10bands' plugin (gst-plugins-good)")?;

        // 5. Optional Soft-Knee Studio Punch Compressor (native F32LE; ratio=1.0 = exact bypass)
        let compressor = gst::ElementFactory::make("audiodynamic")
            .name("studio_punch")
            .property("threshold", 0.42f32)
            .property("ratio", 1.0f32)
            .build()
            .ok();
        if let Some(ref comp) = compressor {
            if comp.has_property("characteristics", None) {
                comp.set_property_from_str("characteristics", "soft-knee");
            }
            if comp.has_property("mode", None) {
                comp.set_property_from_str("mode", "compressor");
            }
        }

        // 6. Optional Acoustic Room Ambience Reverb (native F32LE; level=0.0 = exact dry bypass)
        let room_reverb = gst::ElementFactory::make("freeverb")
            .name("room_reverb")
            .property("level", 0.0f32)
            .property("room-size", 0.35f32)
            .property("damping", 0.55f32)
            .property("width", 1.0f32)
            .build()
            .ok();

        // 7. Stereo Panorama / Balance control
        let balance = gst::ElementFactory::make("audiopanorama")
            .name("balance")
            .property("panorama", 0.0f32)
            .build()
            .context("Missing 'audiopanorama' plugin")?;

        // 8. Real-Time Spectrum Analyzer (placed *before* master volume so visual bars stay independent of speaker volume)
        let spectrum_threshold_db = -60.0f32;
        let spectrum = gst::ElementFactory::make("spectrum")
            .name("spectrum")
            .property("bands", 128u32)
            .property("threshold", spectrum_threshold_db as i32)
            .property("interval", 33_333_333u64) // ~30 FPS updates
            .property("post-messages", true)
            .property("message-magnitude", true)
            .property("message-phase", false)
            .build()
            .context("Missing 'spectrum' plugin")?;

        // 9. Master Volume & Mute
        let volume = gst::ElementFactory::make("volume")
            .name("master_volume")
            .property("volume", 0.8f64)
            .property("mute", false)
            .build()
            .context("Missing 'volume' plugin")?;

        // 10. Optional ReplayGain peak limiter (placed *after* master volume to catch any >1.0 peaks)
        let limiter = gst::ElementFactory::make("rglimiter")
            .name("limiter")
            .build()
            .ok();
        let has_limiter = limiter.is_some();

        // 11. Output conversion with TPDF dithering + noise shaping, then hardware sink
        let convert_out = gst::ElementFactory::make("audioconvert")
            .name("convert_out")
            .build()
            .context("Missing 'audioconvert' plugin")?;
        if convert_out.has_property("dithering", None) {
            convert_out.set_property_from_str("dithering", "tpdf");
        }
        if convert_out.has_property("noise-shaping", None) {
            convert_out.set_property_from_str("noise-shaping", "medium");
        }

        let sink = gst::ElementFactory::make("autoaudiosink")
            .name("audio_sink")
            .build()
            .context("Missing 'autoaudiosink' plugin")?;

        // Build clean modular pure-32-bit-float DSP chain
        let mut chain: Vec<&gst::Element> = vec![
            &convert,
            &dsp_caps,
            &resample,
            &pre_gain,
            &eq,
        ];
        if let Some(ref comp) = compressor {
            chain.push(comp);
        }
        if let Some(ref sw) = stereo_widener {
            chain.push(sw);
        }
        if let Some(ref rev) = room_reverb {
            chain.push(rev);
        }
        chain.push(&balance);
        chain.push(&spectrum);
        chain.push(&volume);
        if let Some(ref lim) = limiter {
            chain.push(lim);
        }
        chain.push(&convert_out);
        chain.push(&sink);

        pipeline.add_many([&src, &decodebin])?;
        pipeline.add_many(&chain)?;

        src.link(&decodebin)?;
        gst::Element::link_many(&chain)?;

        // Dynamically link `decodebin` pad once an audio stream is detected
        let convert_weak = convert.downgrade();
        decodebin.connect_pad_added(move |_, src_pad| {
            let Some(convert) = convert_weak.upgrade() else {
                return;
            };

            let caps = src_pad
                .current_caps()
                .unwrap_or_else(|| src_pad.query_caps(None));

            let is_audio = caps
                .structure(0)
                .map(|s| s.name().starts_with("audio/"))
                .unwrap_or(false);

            if !is_audio {
                return;
            }

            if let Some(sink_pad) = convert.static_pad("sink") {
                if !sink_pad.is_linked() {
                    let _ = src_pad.link(&sink_pad);
                }
            }
        });

        Ok(Self {
            pipeline,
            src,
            pre_gain,
            eq,
            compressor,
            stereo_widener,
            room_reverb,
            balance,
            volume,
            eq_bands: [0.0; 10],
            auto_headroom: true,
            punch_enabled: false,
            stereo_width: 0.0,
            room_reverb_amount: 0.0,
            has_limiter,
            spectrum_threshold_db,
            current_metadata: Mutex::new(TrackMetadata::default()),
        })
    }

    // ================================================================
    // Equalizer & Smart Headroom Management
    // ================================================================

    /// Enables or disables automatic pre-gain attenuation to prevent digital clipping.
    pub fn set_auto_headroom(&mut self, enabled: bool) {
        self.auto_headroom = enabled;
        self.update_headroom();
    }

    pub fn is_auto_headroom(&self) -> bool {
        self.auto_headroom
    }

    /// Recomputes pre-gain attenuation from the highest boosted EQ band
    /// plus optional gentle +1.2 dB make-up drive for Soft-Knee Studio Punch.
    fn update_headroom(&self) {
        let base_gain = if !self.auto_headroom {
            1.0
        } else {
            let max_boost_db = self.eq_bands.iter().copied().fold(0.0_f64, f64::max);
            if max_boost_db > 0.0 {
                10_f64.powf(-max_boost_db / 20.0)
            } else {
                1.0
            }
        };

        let punch_makeup = if self.punch_enabled && self.compressor.is_some() {
            1.15
        } else {
            1.0
        };

        self.pre_gain.set_property("volume", base_gain * punch_makeup);
    }

    pub fn set_eq_band(&mut self, band_index: usize, gain_db: f64) {
        if band_index < 10 {
            let clamped = gain_db.clamp(-12.0, 12.0);
            self.eq_bands[band_index] = clamped;
            let prop_name = format!("band{band_index}");
            self.eq.set_property(&prop_name, clamped);
            self.update_headroom();
        }
    }

    pub fn set_all_eq_bands(&mut self, gains: [f64; 10]) {
        for (i, &gain) in gains.iter().enumerate() {
            let clamped = gain.clamp(-12.0, 12.0);
            self.eq_bands[i] = clamped;
            let prop_name = format!("band{i}");
            self.eq.set_property(&prop_name, clamped);
        }
        self.update_headroom();
    }

    pub fn get_eq_bands(&self) -> &[f64; 10] {
        &self.eq_bands
    }

    pub fn reset_eq(&mut self) {
        self.set_all_eq_bands([0.0; 10]);
    }

    pub fn set_bass_boost(&mut self, amount_db: f64) {
        let amount = amount_db.clamp(0.0, 12.0);
        self.set_eq_band(0, amount * 0.85);
        self.set_eq_band(1, amount * 0.65);
        self.set_eq_band(2, amount * 0.35);
    }

    pub fn set_preset(&mut self, preset: Preset) {
        self.set_all_eq_bands(preset.gains());
    }

    // ================================================================
    // Optional Pure 32-Bit Float Spatial & Dynamics Enhancements
    // ================================================================

    /// Sets pure 32-bit float Mid/Side stereo stage expansion (`0.0` = bit-perfect off, `1.0` = max wide).
    pub fn set_stereo_widening(&mut self, amount: f32) {
        let clamped = amount.clamp(0.0, 1.0);
        self.stereo_width = clamped;
        if let Some(ref sw) = self.stereo_widener {
            sw.set_property("matrix", build_stereo_matrix(clamped));
        }
    }

    pub fn get_stereo_widening(&self) -> f32 {
        self.stereo_width
    }

    pub fn has_stereo_widener(&self) -> bool {
        self.stereo_widener.is_some()
    }

    /// Toggles gentle soft-knee Studio Punch compression (`false` = bit-perfect 1:1 bypass).
    pub fn set_punch_enabled(&mut self, enabled: bool) {
        self.punch_enabled = enabled;
        if let Some(ref comp) = self.compressor {
            comp.set_property("ratio", if enabled { 0.65f32 } else { 1.0f32 });
        }
        self.update_headroom();
    }

    pub fn is_punch_enabled(&self) -> bool {
        self.punch_enabled
    }

    pub fn has_compressor(&self) -> bool {
        self.compressor.is_some()
    }

    /// Sets subtle 32-bit float acoustic room ambience (`0.0` = exact dry bypass, `1.0` = concert hall).
    pub fn set_room_reverb(&mut self, amount: f32) {
        let clamped = amount.clamp(0.0, 1.0);
        self.room_reverb_amount = clamped;
        if let Some(ref rev) = self.room_reverb {
            rev.set_property("level", clamped * 0.28);
            rev.set_property("room-size", 0.25 + clamped * 0.45);
        }
    }

    pub fn get_room_reverb(&self) -> f32 {
        self.room_reverb_amount
    }

    pub fn has_room_reverb(&self) -> bool {
        self.room_reverb.is_some()
    }

    /// Disables all optional DSP enhancements and restores the clean pure 32-bit float baseline.
    pub fn reset_dsp_effects(&mut self) {
        self.set_stereo_widening(0.0);
        self.set_punch_enabled(false);
        self.set_room_reverb(0.0);
        self.set_auto_headroom(true);
    }

    /// Returns `true` when any optional spatial/dynamics enhancement is actively engaged.
    pub fn has_active_enhancements(&self) -> bool {
        self.punch_enabled || self.stereo_width > 0.005 || self.room_reverb_amount > 0.005
    }

    // ================================================================
    // Master Controls (Volume, Mute, Balance)
    // ================================================================

    pub fn set_volume(&self, volume: f64) {
        let max_vol = if self.has_limiter { 1.5 } else { 1.0 };
        self.volume.set_property("volume", volume.clamp(0.0, max_vol));
    }

    pub fn get_volume(&self) -> f64 {
        self.volume.property::<f64>("volume")
    }

    pub fn set_mute(&self, muted: bool) {
        self.volume.set_property("mute", muted);
    }

    pub fn is_muted(&self) -> bool {
        self.volume.property::<bool>("mute")
    }

    /// Sets stereo balance (`-1.0` = full left, `0.0` = center, `1.0` = full right).
    pub fn set_balance(&self, pan: f32) {
        self.balance.set_property("panorama", pan.clamp(-1.0, 1.0));
    }

    // ================================================================
    // Transport & Playback Control
    // ================================================================

    pub fn load_file(&self, filepath: &str) -> Result<()> {
        // Resetting to Null guarantees a clean `decodebin` pad state across formats
        self.pipeline
            .set_state(gst::State::Null)
            .context("Failed to reset pipeline state to Null")?;

        if let Ok(mut meta) = self.current_metadata.lock() {
            *meta = TrackMetadata::default();
        }

        self.src.set_property("location", filepath);
        self.play()
    }

    pub fn play(&self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map(|_| ())
            .map_err(|err| anyhow!("Failed to start playback: {err:?}"))
    }

    pub fn pause(&self) -> Result<()> {
        self.pipeline
            .set_state(gst::State::Paused)
            .map(|_| ())
            .map_err(|err| anyhow!("Failed to pause playback: {err:?}"))
    }

    pub fn stop(&self) -> Result<()> {
        if let Ok(mut meta) = self.current_metadata.lock() {
            *meta = TrackMetadata::default();
        }

        self.pipeline
            .set_state(gst::State::Null)
            .map(|_| ())
            .map_err(|err| anyhow!("Failed to stop playback: {err:?}"))
    }

    pub fn is_playing(&self) -> bool {
        let (_, current, _) = self.pipeline.state(gst::ClockTime::ZERO);
        current == gst::State::Playing
    }

    /// Returns current playback position in fractional seconds.
    pub fn get_position(&self) -> Option<f64> {
        self.pipeline
            .query_position::<gst::ClockTime>()
            .map(|t| t.nseconds() as f64 / 1_000_000_000.0)
    }

    /// Returns total stream duration in fractional seconds.
    pub fn get_duration(&self) -> Option<f64> {
        self.pipeline
            .query_duration::<gst::ClockTime>()
            .map(|t| t.nseconds() as f64 / 1_000_000_000.0)
    }

    /// Performs an accurate flush seek to `position_sec` seconds.
    pub fn seek_position(&self, position_sec: f64) -> Result<()> {
        if !position_sec.is_finite() || position_sec < 0.0 {
            return Err(anyhow!("Invalid seek position: {position_sec}"));
        }
        let position = gst::ClockTime::from_nseconds((position_sec * 1_000_000_000.0) as u64);
        self.pipeline
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, position)
            .map_err(|err| anyhow!("Seek failed: {err:?}"))
    }

    // ================================================================
    // Event Polling for GUI Integration
    // ================================================================

    /// Drains pending messages from the GStreamer bus (~30 FPS from GUI timer).
    pub fn poll_events(&self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        let Some(bus) = self.pipeline.bus() else {
            return events;
        };

        let mut latest_spectrum: Option<Vec<f32>> = None;
        let mut metadata_changed = false;

        while let Some(msg) = bus.pop() {
            match msg.view() {
                gst::MessageView::Eos(..) => {
                    events.push(EngineEvent::EndOfStream);
                }
                gst::MessageView::Error(err) => {
                    let debug_info = err.debug().unwrap_or_default();
                    events.push(EngineEvent::Error(format!(
                        "{} ({})",
                        err.error(),
                        debug_info
                    )));
                }
                gst::MessageView::Tag(tag_msg) => {
                    let tags = tag_msg.tags();
                    if let Ok(mut meta) = self.current_metadata.lock() {
                        if let Some(title) = tags.get::<gst::tags::Title>() {
                            let val = Some(title.get().to_string());
                            if meta.title != val {
                                meta.title = val;
                                metadata_changed = true;
                            }
                        }
                        if let Some(artist) = tags.get::<gst::tags::Artist>() {
                            let val = Some(artist.get().to_string());
                            if meta.artist != val {
                                meta.artist = val;
                                metadata_changed = true;
                            }
                        }
                        if let Some(album) = tags.get::<gst::tags::Album>() {
                            let val = Some(album.get().to_string());
                            if meta.album != val {
                                meta.album = val;
                                metadata_changed = true;
                            }
                        }
                    }
                }
                gst::MessageView::Element(elem) => {
                    if let Some(structure) = elem.structure() {
                        if structure.name() == "spectrum" {
                            if let Ok(mags) = structure.get::<gst::List>("magnitude") {
                                let threshold = self.spectrum_threshold_db;
                                let freqs: Vec<f32> = mags
                                    .as_slice()
                                    .iter()
                                    .filter_map(|val| val.get::<f32>().ok())
                                    .map(|db| ((db - threshold) / -threshold).clamp(0.0, 1.0))
                                    .collect();
                                latest_spectrum = Some(freqs);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if metadata_changed {
            if let Ok(meta) = self.current_metadata.lock() {
                events.push(EngineEvent::MetadataUpdated(meta.clone()));
            }
        }

        if let Some(spec) = latest_spectrum {
            events.push(EngineEvent::Spectrum(spec));
        }

        events
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_pure_32bit_dsp_controls_and_reset() {
        gst::init().expect("gst::init");
        if gst::ElementFactory::find("equalizer-10bands").is_none() {
            // Minimal build container without gst-plugins-good
            return;
        }

        let mut engine = AudioEngine::new().expect("AudioEngine::new should succeed");
        eprintln!(
            "Host DSP plugins detected -> stereo_widener: {}, compressor: {}, room_reverb: {}, limiter: {}",
            engine.has_stereo_widener(),
            engine.has_compressor(),
            engine.has_room_reverb(),
            engine.has_limiter,
        );
        assert!(!engine.has_active_enhancements());
        assert_eq!(engine.get_stereo_widening(), 0.0);
        assert!(!engine.is_punch_enabled());
        assert_eq!(engine.get_room_reverb(), 0.0);
        assert!(engine.is_auto_headroom());

        engine.set_stereo_widening(0.6);
        assert!((engine.get_stereo_widening() - 0.6).abs() < 1e-6);
        assert!(engine.has_active_enhancements());

        engine.set_punch_enabled(true);
        assert!(engine.is_punch_enabled());

        engine.set_room_reverb(0.4);
        assert!((engine.get_room_reverb() - 0.4).abs() < 1e-6);

        engine.reset_dsp_effects();
        assert!(!engine.has_active_enhancements());
        assert_eq!(engine.get_stereo_widening(), 0.0);
        assert!(!engine.is_punch_enabled());
        assert_eq!(engine.get_room_reverb(), 0.0);
        assert!(engine.is_auto_headroom());
    }

    #[test]
    fn live_f32_dsp_pipeline_modulation_while_playing() {
        gst::init().expect("gst::init");
        // Verify live property updates on audiomixmatrix + audiodynamic + freeverb while PLAYING
        let Some(sw) = gst::ElementFactory::make("audiomixmatrix")
            .property("in-channels", 2u32)
            .property("out-channels", 2u32)
            .property("channel-mask", u64::MAX)
            .build()
            .ok()
        else {
            // Minimal container without gst-plugins-bad: skip live plugin stream check
            return;
        };
        sw.set_property_from_str("mode", "manual");
        sw.set_property("matrix", build_stereo_matrix(0.0));

        let src = gst::ElementFactory::make("audiotestsrc")
            .property("num-buffers", 30i32)
            .build()
            .unwrap();
        let conv = gst::ElementFactory::make("audioconvert").build().unwrap();
        let caps = gst::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("channels", 2i32)
            .build();
        let cf = gst::ElementFactory::make("capsfilter")
            .property("caps", &caps)
            .build()
            .unwrap();
        let comp = gst::ElementFactory::make("audiodynamic")
            .property("threshold", 0.42f32)
            .property("ratio", 1.0f32)
            .build()
            .unwrap();
        comp.set_property_from_str("characteristics", "soft-knee");
        comp.set_property_from_str("mode", "compressor");
        let rev = gst::ElementFactory::make("freeverb")
            .property("level", 0.0f32)
            .property("room-size", 0.35f32)
            .property("damping", 0.55f32)
            .property("width", 1.0f32)
            .build()
            .unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();

        let pipe = gst::Pipeline::new();
        pipe.add_many([&src, &conv, &cf, &comp, &sw, &rev, &sink])
            .unwrap();
        gst::Element::link_many([&src, &conv, &cf, &comp, &sw, &rev, &sink]).unwrap();

        pipe.set_state(gst::State::Playing).unwrap();
        let (res, _, _) = pipe.state(gst::ClockTime::from_seconds(2));
        assert!(res.is_ok(), "Pipeline failed to enter Playing state");

        // Live modulate every F32LE DSP block while audio is actively streaming!
        sw.set_property("matrix", build_stereo_matrix(0.65));
        comp.set_property("ratio", 0.65f32);
        rev.set_property("level", 0.16f32);
        rev.set_property("room-size", 0.52f32);
        sw.set_property("matrix", build_stereo_matrix(0.0));
        comp.set_property("ratio", 1.0f32);
        rev.set_property("level", 0.0f32);

        let bus = pipe.bus().unwrap();
        let msg = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &[gst::MessageType::Error, gst::MessageType::Eos],
        );
        pipe.set_state(gst::State::Null).unwrap();
        match msg.map(|m| m.type_()) {
            Some(gst::MessageType::Eos) => {}
            other => panic!("Expected clean EOS during live F32LE DSP test, got {:?}", other),
        }
    }
}
