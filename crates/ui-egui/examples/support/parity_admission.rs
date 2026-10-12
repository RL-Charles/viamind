//! Fixed-size completed-frame settling; repeated UI passes never count as frames.
#[derive(Default)]
pub struct Settling {
    frame: Option<u64>,
    step: usize,
    discarded: bool,
    pub completed: usize,
}
impl Settling {
    pub fn observe(&mut self, frame: u64, step: usize, unchanged: bool, discarded: bool) -> Result<bool, &'static str> {
        if self.step != step {
            *self = Self { step, ..Self::default() };
        }
        if let Some(previous) = self.frame {
            if frame < previous {
                return Err("hardened frame number moved backwards");
            }
            if !unchanged {
                self.completed = 0;
            } else if frame > previous && !self.discarded {
                self.completed = self.completed.saturating_add(1);
            }
        }
        self.frame = Some(frame);
        self.discarded = discarded;
        Ok(self.completed >= 6 && !discarded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settling_counts_completed_frames_not_repeated_or_discarded_passes() {
        let mut s = Settling::default();
        for frame in 0..6 {
            for _ in 0..3 {
                assert!(!s.observe(frame, 0, true, true).unwrap());
            }
            assert!(!s.observe(frame, 0, true, false).unwrap());
        }
        assert_eq!(s.completed, 5);
        assert!(!s.observe(6, 0, true, true).unwrap());
        assert_eq!(s.completed, 6);
        assert!(s.observe(6, 0, true, false).unwrap());
        assert!(!s.observe(6, 0, false, false).unwrap());
        assert_eq!(s.completed, 0);
        assert!(!s.observe(7, 1, true, false).unwrap());
        assert_eq!(s.completed, 0);
        assert!(s.observe(6, 1, true, false).is_err());
    }
}
