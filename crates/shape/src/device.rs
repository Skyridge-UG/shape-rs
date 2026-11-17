//! Device profile: the single source of truth the synthesizer reads values
//! from. One profile describes one coherent browser; fields map 1:1 onto the
//! high-frequency Shape signals so a profile always produces a self-consistent
//! report (no Chrome UA with Safari canvas hashes).

use serde::{Deserialize, Serialize};

/// Coarse platform identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
    Android,
    Ios,
}

/// One coherent synthetic browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub name: String,
    pub platform: Platform,

    /// `navigator.userAgent`, verbatim.
    pub user_agent: String,

    /// `navigator.hardwareConcurrency`.
    pub cpu_cores: u32,

    /// `navigator.deviceMemory` in GiB (the wire rounds to the pow2 ladder).
    pub device_memory_gib: f64,

    /// `screen.width/height` in CSS px.
    pub screen: (u32, u32),

    /// `window.devicePixelRatio`.
    pub device_pixel_ratio: f64,

    /// Primary `Intl.DateTimeFormat().resolvedOptions().timeZone`.
    pub timezone: String,

    /// `navigator.language(s)`, primary first.
    pub languages: Vec<String>,

    /// WebGL vendor/renderer pair (UNMASKED_*).
    pub gpu: GpuIdentity,

    /// Canvas text-rendering fingerprint (the 4 sample words).
    pub canvas_hash: u32,

    /// AudioContext sum of the oscillator probe buffer.
    pub audio_fingerprint: f64,

    /// Font availability bitset indices into the shared candidate list.
    pub fonts_present: Vec<u16>,

    /// User interaction behaviour (keystroke dynamics, mouse travel).
    pub behaviour: BehaviourProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuIdentity {
    pub vendor: String,
    pub renderer: String,
}

/// Behavioural prior: the human-input statistics the collector's event hooks
/// aggregate between init and dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviourProfile {
    /// Keys/sec the keystroke sampler should see.
    pub keystrokes_per_sec: f64,

    /// Dwell ms distribution (mean, sigma).
    pub key_dwell_ms: (f64, f64),

    /// Mouse px/s travel prior.
    pub mouse_px_per_sec: f64,

    /// Clicks per minute and scroll ticks per minute.
    pub clicks_per_min: u32,
    pub scroll_per_min: u32,
}

impl DeviceProfile {
    /// Coherent defaults: a mid-range Windows 11 desktop on Chrome.
    pub fn win11_chrome() -> Self {
        Self {
            name: "win11-chrome-stable".into(),
            platform: Platform::Windows,
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36".into(),
            cpu_cores: 16,
            device_memory_gib: 8.0,
            screen: (2560, 1440),
            device_pixel_ratio: 1.0,
            timezone: "Europe/Berlin".into(),
            languages: vec!["en-US".into(), "de".into()],
            gpu: GpuIdentity {
                vendor: "Google Inc. (NVIDIA)".into(),
                renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 (0x00002503) Direct3D11 vs_5_0 ps_5_0, D3D11)".into(),
            },
            canvas_hash: 0x5ecd_ab12,
            audio_fingerprint: 124.043_475_278_459_23,
            fonts_present: vec![0, 1, 2, 5, 8],
            behaviour: BehaviourProfile {
                keystrokes_per_sec: 5.2,
                key_dwell_ms: (85.0, 22.0),
                mouse_px_per_sec: 640.0,
                clicks_per_min: 9,
                scroll_per_min: 14,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_self_consistent() {
        let p = DeviceProfile::win11_chrome();
        assert!(p.cpu_cores >= 1 && p.cpu_cores <= 64);
        assert!(p.device_memory_gib >= 0.5 && p.device_memory_gib <= 32.0);
        assert!(p.screen.0 >= 640 && p.screen.1 >= 480);
        assert!(p.device_pixel_ratio >= 0.5);
        assert_eq!(p.platform, Platform::Windows);
    }
}
