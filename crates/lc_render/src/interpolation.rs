//! # Roblox-style Snapshot Interpolation Buffer
//!
//! Maintains a sliding window of timestamped `ClientState` snapshots (received at 20 TPS),
//! and samples smooth linear interpolations (`lerp`) at arbitrary render timestamps (60~144 FPS).

use std::collections::VecDeque;
use lc_core::{ClientState, EntityDesc};

/// A timestamped snapshot entry.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotEntry {
    pub time: f64,
    pub state: ClientState,
}

/// A sliding buffer storing recent snapshots for smooth entity interpolation.
#[derive(Debug, Clone)]
pub struct SnapshotInterpolationBuffer {
    snapshots: VecDeque<SnapshotEntry>,
    max_history_seconds: f64,
}

impl Default for SnapshotInterpolationBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotInterpolationBuffer {
    /// Creates a new interpolation buffer with default 3.0 second retention.
    pub fn new() -> Self {
        Self {
            snapshots: VecDeque::with_capacity(128),
            max_history_seconds: 3.0,
        }
    }

    /// Creates an interpolation buffer with a custom history window in seconds.
    pub fn with_history_seconds(max_history_seconds: f64) -> Self {
        Self {
            snapshots: VecDeque::with_capacity(128),
            max_history_seconds,
        }
    }

    /// Pushes a new server snapshot with its local arrival timestamp.
    pub fn push(&mut self, time: f64, state: ClientState) {
        if let Some(last) = self.snapshots.back() {
            if time <= last.time {
                // Out of order packet: insert in chronological order
                let idx = self.snapshots.iter().position(|s| s.time >= time);
                if let Some(i) = idx {
                    self.snapshots.insert(i, SnapshotEntry { time, state });
                } else {
                    self.snapshots.push_back(SnapshotEntry { time, state });
                }
            } else {
                self.snapshots.push_back(SnapshotEntry { time, state });
            }
        } else {
            self.snapshots.push_back(SnapshotEntry { time, state });
        }

        // Prune old snapshots beyond the max history window
        if let Some(latest) = self.snapshots.back() {
            let cutoff = latest.time - self.max_history_seconds;
            while self.snapshots.len() > 2 {
                if let Some(front) = self.snapshots.front() {
                    if front.time < cutoff {
                        self.snapshots.pop_front();
                        continue;
                    }
                }
                break;
            }
        }
    }

    /// Samples interpolated entity positions at the requested `render_time` (typically `now - 50ms`).
    ///
    /// Locates the two bounding snapshots `S0` and `S1` such that `S0.time <= render_time <= S1.time`,
    /// and performs linear interpolation on entity positions.
    pub fn sample(&self, render_time: f64) -> Option<Vec<EntityDesc>> {
        if self.snapshots.is_empty() {
            return None;
        }

        if self.snapshots.len() == 1 {
            return Some(self.snapshots[0].state.entities.clone());
        }

        let first = self.snapshots.front().unwrap();
        let last = self.snapshots.back().unwrap();

        // If render_time <= oldest snapshot, clamp to oldest
        if render_time <= first.time {
            return Some(first.state.entities.clone());
        }

        // If render_time >= newest snapshot, clamp to newest
        if render_time >= last.time {
            return Some(last.state.entities.clone());
        }

        // Find two bounding snapshots: s0.time <= render_time <= s1.time
        let mut s0 = first;
        let mut s1 = last;

        let snapshot_vec: Vec<&SnapshotEntry> = self.snapshots.iter().collect();
        for window in snapshot_vec.windows(2) {
            let prev = window[0];
            let next = window[1];
            if prev.time <= render_time && render_time <= next.time {
                s0 = prev;
                s1 = next;
                break;
            }
        }

        let time_delta = s1.time - s0.time;
        let alpha = if time_delta > 1e-6 {
            ((render_time - s0.time) / time_delta).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        // Linearly interpolate positions of entities present in s1
        let mut result = Vec::with_capacity(s1.state.entities.len());
        for ent1 in &s1.state.entities {
            let mut interpolated = ent1.clone();
            if let Some(ent0) = s0.state.entities.iter().find(|e| e.id == ent1.id) {
                interpolated.pos[0] = ent0.pos[0] + (ent1.pos[0] - ent0.pos[0]) * alpha;
                interpolated.pos[1] = ent0.pos[1] + (ent1.pos[1] - ent0.pos[1]) * alpha;
                interpolated.pos[2] = ent0.pos[2] + (ent1.pos[2] - ent0.pos[2]) * alpha;
            }
            result.push(interpolated);
        }

        Some(result)
    }

    /// Samples interpolated camera target coordinate at `render_time`.
    pub fn sample_camera_target(&self, render_time: f64) -> Option<[f32; 3]> {
        if self.snapshots.is_empty() {
            return None;
        }

        if self.snapshots.len() == 1 {
            return Some(self.snapshots[0].state.camera_target);
        }

        let first = self.snapshots.front().unwrap();
        let last = self.snapshots.back().unwrap();

        if render_time <= first.time {
            return Some(first.state.camera_target);
        }
        if render_time >= last.time {
            return Some(last.state.camera_target);
        }

        let mut s0 = first;
        let mut s1 = last;

        let snapshot_vec: Vec<&SnapshotEntry> = self.snapshots.iter().collect();
        for window in snapshot_vec.windows(2) {
            let prev = window[0];
            let next = window[1];
            if prev.time <= render_time && render_time <= next.time {
                s0 = prev;
                s1 = next;
                break;
            }
        }

        let time_delta = s1.time - s0.time;
        let alpha = if time_delta > 1e-6 {
            ((render_time - s0.time) / time_delta).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        let t0 = s0.state.camera_target;
        let t1 = s1.state.camera_target;
        Some([
            t0[0] + (t1[0] - t0[0]) * alpha,
            t0[1] + (t1[1] - t0[1]) * alpha,
            t0[2] + (t1[2] - t0[2]) * alpha,
        ])
    }

    /// Returns a reference to the latest received `ClientState`, if any.
    pub fn latest(&self) -> Option<&ClientState> {
        self.snapshots.back().map(|s| &s.state)
    }

    /// Number of snapshots currently in the buffer.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Checks if the buffer contains no snapshots.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Clears all snapshots in the buffer.
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }
}
