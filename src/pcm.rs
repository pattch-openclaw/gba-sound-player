//! PCM Audio playback via Direct Sound and DMA

#[macro_export]
macro_rules! include_aligned_bytes {
    ($file:expr) => {{
        #[repr(C, align(4))]
        struct Aligned<const N: usize>(pub [u8; N]);
        static ALIGNED: Aligned<{ include_bytes!($file).len() }> = Aligned(*include_bytes!($file));
        &ALIGNED.0
    }};
}

pub struct PcmTrack {
    pub left_data: &'static [u8],
    pub right_data: &'static [u8],
    pub sample_rate: u32,
}

impl PcmTrack {
    pub const fn new(left_data: &'static [u8], right_data: &'static [u8], sample_rate: u32) -> Self {
        assert!(left_data.len() == right_data.len(), "left_data and right_data must have the same length");
        Self {
            left_data,
            right_data,
            sample_rate,
        }
    }

    pub fn length(&self) -> usize {
        self.left_data.len()
    }
}

pub fn play_stereo_track_blocking(track: &PcmTrack) {
    // [Truncated for brevity in test check]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_pcm_track_creation(_gba: &mut agb::Gba) {
        static LEFT: [u8; 32] = [0; 32];
        static RIGHT: [u8; 32] = [0; 32];
        let track = PcmTrack::new(&LEFT, &RIGHT, 44100);
        assert_eq!(track.length(), 32);
        assert_eq!(track.sample_rate, 44100);
    }
}
