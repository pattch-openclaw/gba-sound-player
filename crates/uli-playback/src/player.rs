//! The player: the output stage in front of the ring, and the ledger the gate reads.
//!
//! `RingFeed` is `gba_playback::playback::RingFeed`, linked rather than
//! re-invented — the same type the FLAC product path and the album player use.
//! What this module adds is the two things ULI2's player owes on top of it:
//!
//! * **The output stage is inside `feed`**, so a speaker build cannot forget to
//!   fold: `feed(l, r, …)` folds per the player's [`Profile`] and writes FIFO
//!   bytes, and the ring only ever holds what the hardware will play.
//! * **A ledger with the gate's vocabulary** ([`Ledger`]): periods served, a
//!   starved period *while the producer had work* (an underrun — the judge), and
//!   a starved period *after the producer finished* (a tail, not a fault).
//!
//! The caller owns the interrupt discipline: `feed` runs in the producer's
//! critical section (the ring's copy), `take` in the feeder's — exactly as
//! `gba-playback`'s module docs require, because `thumbv4t-none-eabi` has no
//! atomics.

use gba_playback::playback::{Chunk, PushError, RingFeed};

use crate::fold::{FoldHow, Profile, fold_into};

/// The player's running accounting. All plain integers: written inside the
/// caller's critical sections, never across one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Feeder periods entered (one per timer-IRQ service).
    pub periods: u32,
    /// Periods fully served from the ring.
    pub served: u32,
    /// Periods starved **while the producer had not finished**: the underrun
    /// count, and the judge of a playback claim.
    pub underruns: u32,
    /// Periods starved **after** the producer finished: the tail, which is not a
    /// fault.
    pub tail_after_end: u32,
    /// Frames the producer folded and queued.
    pub frames_fed: u32,
    /// Largest number of samples ever queued after a `feed` — the ring's high
    /// water, which is what says the prefill depth is adequate.
    pub max_ready: usize,
}

impl Ledger {
    /// Fold one feeder result in. `finished` is the producer's own flag, which is
    /// the only thing that distinguishes a tail from an underrun.
    pub fn record(&mut self, chunk: Chunk, finished: bool) {
        self.periods = self.periods.saturating_add(1);
        match chunk {
            Chunk::Full => self.served = self.served.saturating_add(1),
            Chunk::Tail { .. } => self.tail_after_end = self.tail_after_end.saturating_add(1),
            Chunk::Starved => {
                if finished {
                    self.tail_after_end = self.tail_after_end.saturating_add(1);
                } else {
                    self.underruns = self.underruns.saturating_add(1);
                }
            }
        }
    }

    /// True when the gate's audio claim holds: every period was served from the
    /// ring while the producer had work.
    pub fn clean(&self) -> bool {
        self.underruns == 0 && self.periods > 0
    }
}

/// A player over a ring of `RING` samples.
pub struct Player<const RING: usize> {
    /// The ring itself, public so the feeder can take from it in its handler.
    pub feed: RingFeed<RING>,
    /// Headphones (stereo as decoded) or the speaker (folded in `feed`).
    pub profile: Profile,
    /// Which fold a speaker build uses.
    pub how: FoldHow,
    /// The accounting.
    pub ledger: Ledger,
}

impl<const RING: usize> Player<RING> {
    /// A player with an empty ring.
    pub const fn new(profile: Profile, how: FoldHow) -> Self {
        Self {
            feed: RingFeed::new(),
            profile,
            how,
            ledger: Ledger {
                periods: 0,
                served: 0,
                underruns: 0,
                tail_after_end: 0,
                frames_fed: 0,
                max_ready: 0,
            },
        }
    }

    /// Fold one decoded frame and queue it. `scratch_l`/`scratch_r` must be at
    /// least `l.len()` long; they are the caller's statics so this stays
    /// allocation-free.
    pub fn feed(
        &mut self,
        l: &[i8],
        r: &[i8],
        scratch_l: &mut [u8],
        scratch_r: &mut [u8],
    ) -> Result<(), PushError> {
        fold_into(l, r, scratch_l, scratch_r, self.profile, self.how);
        self.feed
            .push_bytes(&scratch_l[..l.len()], &scratch_r[..l.len()])?;
        self.ledger.frames_fed = self.ledger.frames_fed.saturating_add(1);
        self.ledger.max_ready = self.ledger.max_ready.max(self.feed.ready());
        Ok(())
    }

    /// One feeder period: copy up to `dst_l.len()` samples out, classifying the
    /// result into the ledger. `finished` is the producer's flag.
    pub fn take(&mut self, dst_l: &mut [u8], dst_r: &mut [u8], finished: bool) -> Chunk {
        let chunk = self.feed.take_chunk(dst_l, dst_r, dst_l.len(), finished);
        self.ledger.record(chunk, finished);
        chunk
    }

    /// Samples queued and not yet taken.
    pub fn ready(&self) -> usize {
        self.feed.ready()
    }

    /// Samples the ring can still accept.
    pub fn free(&self) -> usize {
        self.feed.free_samples()
    }
}

#[cfg(test)]
mod tests {
    use gba_playback::playback::Chunk;

    use super::{FoldHow, Player, Profile};

    fn frame(value: i8) -> ([i8; 512], [i8; 512]) {
        ([value; 512], [value; 512])
    }

    /// The ledger's three states, which are the gate's whole vocabulary.
    #[test]
    fn the_ledger_separates_served_underrun_and_tail() {
        let mut p: Player<2048> = Player::new(Profile::Headphone, FoldHow::Average);
        let (l, r) = frame(10);
        let (mut sl, mut sr) = ([0u8; 512], [0u8; 512]);
        let (mut dl, mut dr) = ([0u8; 512], [0u8; 512]);

        p.feed(&l, &r, &mut sl, &mut sr).expect("a frame fits");
        assert!(matches!(p.take(&mut dl, &mut dr, false), Chunk::Full));
        // Nothing left and the producer has not finished: an underrun.
        assert!(matches!(p.take(&mut dl, &mut dr, false), Chunk::Starved));
        // Nothing left and the producer HAS finished: a tail, not a fault.
        assert!(matches!(p.take(&mut dl, &mut dr, true), Chunk::Starved));

        assert_eq!(p.ledger.periods, 3);
        assert_eq!(p.ledger.served, 1);
        assert_eq!(p.ledger.underruns, 1);
        assert_eq!(p.ledger.tail_after_end, 1);
        assert!(!p.ledger.clean());
    }

    /// A clean run is what the gate asks for: every period served while the
    /// producer still had work.
    #[test]
    fn a_paced_run_is_clean() {
        let mut p: Player<4096> = Player::new(Profile::Headphone, FoldHow::Average);
        let (l, r) = frame(0);
        let (mut sl, mut sr) = ([0u8; 512], [0u8; 512]);
        let (mut dl, mut dr) = ([0u8; 512], [0u8; 512]);
        for _ in 0..4 {
            p.feed(&l, &r, &mut sl, &mut sr).expect("fits");
            assert!(matches!(p.take(&mut dl, &mut dr, false), Chunk::Full));
        }
        assert_eq!(p.ledger.underruns, 0);
        assert!(p.ledger.clean());
        // Fed and taken alternately, so the queue never exceeds one frame.
        assert_eq!(
            p.ledger.max_ready, 512,
            "one frame is the high water when paced"
        );
    }

    /// The speaker profile folds *before* the ring, so what the feeder plays is
    /// already mono — both FIFO streams equal, and equal to the fold.
    #[test]
    fn the_speaker_fold_happens_before_the_ring() {
        let mut p: Player<2048> = Player::new(Profile::Speaker, FoldHow::Average);
        let l = [100i8; 512];
        let r = [-100i8; 512];
        let (mut sl, mut sr) = ([0u8; 512], [0u8; 512]);
        p.feed(&l, &r, &mut sl, &mut sr).expect("fits");
        let (mut dl, mut dr) = ([0u8; 512], [0u8; 512]);
        assert!(matches!(p.take(&mut dl, &mut dr, false), Chunk::Full));
        assert_eq!(dl, dr, "mono reaches both FIFOs");
        assert!(dl.iter().all(|&b| b == 0), "(100 + -100) >> 1 = 0");
    }
}
