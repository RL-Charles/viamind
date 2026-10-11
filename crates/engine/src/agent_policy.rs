//! Trusted transient admission shared by every endpoint and the human desktop.
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

pub const MAX_AGENT_WRITERS: usize = 16;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    #[default]
    Live,
    Approval,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentEffect {
    Observation,
    PrivateCandidate,
    LiveWriter,
}
impl AgentMode {
    pub fn admits(self, effect: AgentEffect) -> bool {
        matches!(effect, AgentEffect::Observation) || self == Self::Live || (self == Self::Approval && effect == AgentEffect::PrivateCandidate)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AgentModeStatus {
    pub mode: AgentMode,
    pub generation: u64,
    pub startup_locked: bool,
    pub active_writers: usize,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AgentPolicyError {
    #[error("agent operation is unavailable in the current mode")]
    Forbidden,
    #[error("agents are permanently read only by startup restriction")]
    StartupLocked,
    #[error("{0} agent operations are unfinished; retry the mode change after completion")]
    Busy(usize),
    #[error("16 agent operations are already active; retry after completion")]
    Capacity,
    #[error("agent policy generation exhausted")]
    GenerationExhausted,
}

#[derive(Debug)]
pub struct AgentPolicy {
    state: Mutex<AgentModeStatus>,
    changes: watch::Sender<u64>,
}
impl Default for AgentPolicy {
    fn default() -> Self {
        Self {
            state: Mutex::new(AgentModeStatus { mode: AgentMode::Live, generation: 0, startup_locked: false, active_writers: 0 }),
            changes: watch::channel(0).0,
        }
    }
}
impl AgentPolicy {
    fn state(&self) -> std::sync::MutexGuard<'_, AgentModeStatus> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub fn status(&self) -> AgentModeStatus {
        self.state().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
    pub fn set_mode(&self, mode: AgentMode) -> Result<AgentModeStatus, AgentPolicyError> {
        self.transition(mode, false)
    }
    /// Trusted startup path only; this restriction can never be lifted.
    pub fn lock_read_only(&self) -> Result<AgentModeStatus, AgentPolicyError> {
        self.transition(AgentMode::ReadOnly, true)
    }
    fn transition(&self, mode: AgentMode, lock: bool) -> Result<AgentModeStatus, AgentPolicyError> {
        let result = {
            let mut state = self.state();
            if state.startup_locked && mode != AgentMode::ReadOnly {
                return Err(AgentPolicyError::StartupLocked);
            }
            if state.mode == mode && (!lock || state.startup_locked) {
                return Ok(state.clone());
            }
            if state.active_writers != 0 {
                return Err(AgentPolicyError::Busy(state.active_writers));
            }
            let next = state.generation.checked_add(1).ok_or(AgentPolicyError::GenerationExhausted)?;
            state.mode = mode;
            state.startup_locked |= lock;
            state.generation = next;
            state.clone()
        };
        // Coalesce notifications monotonically even if two human calls race after unlocking.
        self.changes.send_if_modified(|generation| {
            if *generation < result.generation {
                *generation = result.generation;
                true
            } else {
                false
            }
        });
        Ok(result)
    }
    pub fn admit(self: &Arc<Self>, effect: AgentEffect) -> Result<Option<AgentWriterPermit>, AgentPolicyError> {
        let mut state = self.state();
        if !state.mode.admits(effect) {
            return Err(AgentPolicyError::Forbidden);
        }
        if effect == AgentEffect::Observation {
            return Ok(None);
        }
        if state.active_writers >= MAX_AGENT_WRITERS {
            return Err(AgentPolicyError::Capacity);
        }
        state.active_writers += 1;
        Ok(Some(AgentWriterPermit { policy: Arc::clone(self) }))
    }
}

/// Move into the actual retained handler task, never only the transport awaiter.
#[derive(Debug)]
pub struct AgentWriterPermit {
    policy: Arc<AgentPolicy>,
}
impl Drop for AgentWriterPermit {
    fn drop(&mut self) {
        let mut state = self.policy.state();
        debug_assert!(state.active_writers > 0);
        state.active_writers -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_admission_blocks_transition_capacity_and_immutable_startup_survives_clones() {
        let policy = Arc::new(AgentPolicy::default());
        let held: Vec<_> = (0..MAX_AGENT_WRITERS).map(|_| policy.admit(AgentEffect::LiveWriter).unwrap().unwrap()).collect();
        assert_eq!(policy.admit(AgentEffect::LiveWriter).unwrap_err(), AgentPolicyError::Capacity);
        assert_eq!(policy.set_mode(AgentMode::Approval).unwrap_err(), AgentPolicyError::Busy(16));
        assert_eq!(policy.set_mode(AgentMode::Live).unwrap().generation, 0);
        drop(held);
        assert_eq!(policy.set_mode(AgentMode::Approval).unwrap().generation, 1);
        assert_eq!(policy.admit(AgentEffect::LiveWriter).unwrap_err(), AgentPolicyError::Forbidden);
        drop(policy.admit(AgentEffect::PrivateCandidate).unwrap());
        assert_eq!(policy.set_mode(AgentMode::ReadOnly).unwrap().generation, 2);
        assert!(policy.admit(AgentEffect::Observation).unwrap().is_none());
        assert_eq!(policy.admit(AgentEffect::PrivateCandidate).unwrap_err(), AgentPolicyError::Forbidden);
        policy.set_mode(AgentMode::Live).unwrap();
        policy.lock_read_only().unwrap();
        let clone = Arc::clone(&policy);
        assert_eq!(clone.set_mode(AgentMode::Live).unwrap_err(), AgentPolicyError::StartupLocked);
        assert!(clone.status().startup_locked);
    }
    #[test]
    fn real_concurrent_admission_and_restriction_have_one_legal_ordering() {
        for _ in 0..32 {
            let policy = Arc::new(AgentPolicy::default());
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let child_policy = Arc::clone(&policy);
            let child_barrier = Arc::clone(&barrier);
            let child = std::thread::spawn(move || {
                child_barrier.wait();
                child_policy.admit(AgentEffect::LiveWriter)
            });
            barrier.wait();
            let transition = policy.set_mode(AgentMode::ReadOnly);
            let admission = child.join().unwrap();
            match (transition, admission) {
                (Ok(status), Err(AgentPolicyError::Forbidden)) => assert_eq!(status.mode, AgentMode::ReadOnly),
                (Err(AgentPolicyError::Busy(1)), Ok(Some(permit))) => {
                    assert_eq!(policy.status().mode, AgentMode::Live);
                    drop(permit);
                }
                pair => panic!("illegal admission order: {pair:?}"),
            }
            assert_eq!(policy.status().active_writers, 0);
        }
    }
    #[test]
    fn generation_overflow_preserves_every_policy_field() {
        let policy = AgentPolicy::default();
        policy.state().generation = u64::MAX;
        let before = policy.status();
        assert_eq!(policy.set_mode(AgentMode::Approval).unwrap_err(), AgentPolicyError::GenerationExhausted);
        assert_eq!(policy.status(), before);
    }
}
