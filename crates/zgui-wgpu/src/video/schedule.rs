//! Which queued video frame a refresh shows, and when the next one is due.
//!
//! A frame stamped with a presentation time belongs to the refresh nearest that time. A refresh
//! at `T` with interval `i` therefore shows the latest frame stamped at or before `T + i/2`, and
//! every earlier frame was never going to be shown. The next frame is due for the refresh nearest
//! its own stamp, so the loop needs to wake half an interval before it and no earlier.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::VideoFrame;

/// How many timed frames one surface holds before the earliest is dropped.
pub(crate) const CAPACITY: usize = 8;

/// Timed frames, earliest first.
#[derive(Default)]
pub(crate) struct Queue {
    /// The frames and their stamps, sorted by stamp.
    frames: VecDeque<(Instant, VideoFrame)>,
}

impl Queue {
    /// Files `frame` under `at`. A frame already filed under the same moment is replaced, and the
    /// earliest frame leaves when the queue is full. Returns what left.
    pub(crate) fn insert(&mut self, at: Instant, frame: VideoFrame) -> Vec<VideoFrame> {
        let mut left = Vec::new();
        let index = self.frames.partition_point(|(stamp, _)| *stamp < at);
        if self
            .frames
            .get(index)
            .is_some_and(|(stamp, _)| *stamp == at)
        {
            let (_, old) = std::mem::replace(&mut self.frames[index], (at, frame));
            left.push(old);
        } else {
            self.frames.insert(index, (at, frame));
        }
        while self.frames.len() > CAPACITY {
            left.extend(self.frames.pop_front().map(|(_, frame)| frame));
        }
        left
    }

    /// Empties the queue.
    pub(crate) fn clear(&mut self) -> Vec<VideoFrame> {
        self.frames.drain(..).map(|(_, frame)| frame).collect()
    }

    /// Takes the frame the refresh at `presents_at` shows, if one came due, and every earlier
    /// frame, which no refresh will show.
    pub(crate) fn take_due(
        &mut self,
        presents_at: Instant,
        interval: Duration,
    ) -> (Option<VideoFrame>, Vec<VideoFrame>) {
        let horizon = presents_at + interval / 2;
        let due = self.frames.partition_point(|(stamp, _)| *stamp <= horizon);
        let mut taken: Vec<VideoFrame> = self.frames.drain(..due).map(|(_, f)| f).collect();
        let shown = taken.pop();
        (shown, taken)
    }

    /// When the loop has to wake for the next frame: half an interval before its stamp.
    pub(crate) fn wake_at(&self, interval: Duration) -> Option<Instant> {
        self.frames
            .front()
            .map(|(stamp, _)| stamp.checked_sub(interval / 2).unwrap_or(*stamp))
    }

    /// Every queued frame.
    pub(crate) fn frames(&self) -> impl Iterator<Item = &VideoFrame> {
        self.frames.iter().map(|(_, frame)| frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::ColorSpace;
    use crate::video::testing::{device, i420};

    const INTERVAL: Duration = Duration::from_millis(16);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_refresh_shows_the_latest_frame_nearest_it_and_drops_the_earlier_ones() {
        let Some((gpu, _held)) = device() else { return };
        let start = Instant::now();
        let mut queue = Queue::default();
        for stamp in [0, 33, 66, 100] {
            let frame =
                VideoFrame::new(i420(&gpu), ColorSpace::BT709).with_peak_luminance(stamp as f32);
            assert!(queue.insert(start + ms(stamp), frame).is_empty());
        }
        // A refresh at 60 ms reaches to 68 ms: the 66 ms frame shows, two never will.
        let (shown, dropped) = queue.take_due(start + ms(60), INTERVAL);
        assert_eq!(shown.and_then(|f| f.peak), Some(66.0));
        assert_eq!(dropped.len(), 2);
        // The 100 ms frame is due for the refresh nearest it: wake 8 ms ahead.
        assert_eq!(queue.wake_at(INTERVAL), Some(start + ms(92)));
        let (shown, _) = queue.take_due(start + ms(76), INTERVAL);
        assert!(shown.is_none(), "76 ms + 8 ms does not reach 100 ms");
        let (shown, _) = queue.take_due(start + ms(93), INTERVAL);
        assert_eq!(shown.and_then(|f| f.peak), Some(100.0));
        assert_eq!(queue.wake_at(INTERVAL), None);
    }

    #[test]
    fn a_full_queue_drops_its_earliest_frame_and_a_restamp_replaces() {
        let Some((gpu, _held)) = device() else { return };
        let start = Instant::now();
        let mut queue = Queue::default();
        let frame = || VideoFrame::new(i420(&gpu), ColorSpace::BT709);
        for n in 0..CAPACITY as u64 {
            assert!(queue.insert(start + ms(10 * n), frame()).is_empty());
        }
        assert_eq!(queue.insert(start + ms(10), frame()).len(), 1, "same stamp");
        assert_eq!(
            queue.insert(start + ms(500), frame()).len(),
            1,
            "over capacity"
        );
        assert_eq!(queue.frames().count(), CAPACITY);
        assert_eq!(queue.wake_at(INTERVAL), Some(start + ms(10) - INTERVAL / 2));
    }
}
