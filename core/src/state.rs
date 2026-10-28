//! Session State Machine & Deterministic Lifecycle Transitions
//! Implements the 12+ canonical session states and transition rules from the Master Blueprint (Section 5.4).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

/// The canonical session states for peer-to-peer USB virtualization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionState {
    Offline,
    Initializing,
    Ready,
    Pairing,
    Negotiating,
    Connecting,
    PreparingDevice,
    Active,
    Degraded,
    Recovering,
    Interrupted,
    Closing,
    Failed,
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Error returned when an illegal session state transition is attempted
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransitionError {
    pub from: SessionState,
    pub to: SessionState,
    pub message: String,
}

impl fmt::Display for StateTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Illegal session state transition from {} to {}: {}",
            self.from, self.to, self.message
        )
    }
}

impl std::error::Error for StateTransitionError {}

/// Deterministic transition validator following Master Blueprint Section 5.4
pub fn can_transition(from: SessionState, to: SessionState) -> bool {
    if from == to {
        return true; // Self-transitions are idempotent no-ops
    }

    match from {
        SessionState::Offline => matches!(
            to,
            SessionState::Initializing | SessionState::Failed
        ),
        SessionState::Initializing => matches!(
            to,
            SessionState::Ready | SessionState::Offline | SessionState::Failed
        ),
        SessionState::Ready => matches!(
            to,
            SessionState::Pairing | SessionState::Connecting | SessionState::Offline | SessionState::Failed
        ),
        SessionState::Pairing => matches!(
            to,
            SessionState::Negotiating | SessionState::Ready | SessionState::Closing | SessionState::Failed
        ),
        SessionState::Negotiating => matches!(
            to,
            SessionState::Connecting | SessionState::Ready | SessionState::Closing | SessionState::Failed
        ),
        SessionState::Connecting => matches!(
            to,
            SessionState::PreparingDevice | SessionState::Ready | SessionState::Closing | SessionState::Failed
        ),
        SessionState::PreparingDevice => matches!(
            to,
            SessionState::Active | SessionState::Ready | SessionState::Degraded | SessionState::Closing | SessionState::Failed
        ),
        SessionState::Active => matches!(
            to,
            SessionState::Degraded | SessionState::Interrupted | SessionState::PreparingDevice | SessionState::Closing | SessionState::Failed
        ),
        SessionState::Degraded => matches!(
            to,
            SessionState::Active | SessionState::Interrupted | SessionState::Closing | SessionState::Failed
        ),
        SessionState::Interrupted => matches!(
            to,
            SessionState::Recovering | SessionState::Closing | SessionState::Failed
        ),
        SessionState::Recovering => matches!(
            to,
            SessionState::Active | SessionState::Degraded | SessionState::Closing | SessionState::Ready | SessionState::Failed
        ),
        SessionState::Closing => matches!(
            to,
            SessionState::Ready | SessionState::Offline | SessionState::Failed
        ),
        SessionState::Failed => matches!(
            to,
            SessionState::Ready | SessionState::Initializing | SessionState::Offline
        ),
    }
}

/// Returns the timeout duration for transitory states defined in Master Blueprint:
/// Pairing: 30s timeout
/// Negotiating: 15s timeout
/// Connecting: 20s timeout
/// Recovering: 10s timeout
pub fn transitory_timeout(state: SessionState) -> Option<Duration> {
    match state {
        SessionState::Pairing => Some(Duration::from_secs(30)),
        SessionState::Negotiating => Some(Duration::from_secs(15)),
        SessionState::Connecting => Some(Duration::from_secs(20)),
        SessionState::Recovering => Some(Duration::from_secs(10)),
        _ => None,
    }
}

/// Session State Machine managing current state, transitions, and transitory timeouts
#[derive(Debug, Clone)]
pub struct SessionStateMachine {
    current_state: SessionState,
    previous_state: Option<SessionState>,
    last_reason: Option<String>,
    state_entered_at: Instant,
}

impl Default for SessionStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStateMachine {
    /// Creates a new SessionStateMachine initialized to `Offline`
    pub fn new() -> Self {
        Self {
            current_state: SessionState::Offline,
            previous_state: None,
            last_reason: None,
            state_entered_at: Instant::now(),
        }
    }

    /// Creates a new SessionStateMachine initialized to a specific state
    pub fn with_initial_state(initial: SessionState) -> Self {
        Self {
            current_state: initial,
            previous_state: None,
            last_reason: None,
            state_entered_at: Instant::now(),
        }
    }

    /// Returns the current session state
    pub fn current_state(&self) -> SessionState {
        self.current_state
    }

    /// Returns the previous session state, if any transition has occurred
    pub fn previous_state(&self) -> Option<SessionState> {
        self.previous_state
    }

    /// Returns the reason for the last state transition
    pub fn last_reason(&self) -> Option<&str> {
        self.last_reason.as_deref()
    }

    /// Returns the instant when the current state was entered
    pub fn state_entered_at(&self) -> Instant {
        self.state_entered_at
    }

    /// Returns the duration spent in the current state
    pub fn time_in_current_state(&self) -> Duration {
        self.state_entered_at.elapsed()
    }

    /// Checks if a transition to `next` is valid from the current state
    pub fn can_transition(&self, next: SessionState) -> bool {
        can_transition(self.current_state, next)
    }

    /// Transitions to `next` if valid, recording the transition timestamp and optional reason
    pub fn transition_to(
        &mut self,
        next: SessionState,
        reason: Option<String>,
    ) -> Result<(), StateTransitionError> {
        if !self.can_transition(next) {
            return Err(StateTransitionError {
                from: self.current_state,
                to: next,
                message: format!(
                    "Invalid transition: cannot move from {:?} to {:?}",
                    self.current_state, next
                ),
            });
        }

        self.previous_state = Some(self.current_state);
        self.current_state = next;
        self.last_reason = reason;
        self.state_entered_at = Instant::now();
        Ok(())
    }

    /// Returns the timeout duration configured for the current state, if transitory
    pub fn timeout_duration(&self) -> Option<Duration> {
        transitory_timeout(self.current_state)
    }

    /// Checks if the current state is a transitory state with a configured timeout
    pub fn is_transitory(&self) -> bool {
        self.timeout_duration().is_some()
    }

    /// Checks if the current transitory state has exceeded its configured timeout
    pub fn is_timed_out(&self) -> bool {
        match self.timeout_duration() {
            Some(timeout) => self.state_entered_at.elapsed() >= timeout,
            None => false,
        }
    }

    /// Overrides the state entered timestamp (useful for synthetic timeout testing)
    pub fn set_state_entered_at(&mut self, instant: Instant) {
        self.state_entered_at = instant;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_offline() {
        let sm = SessionStateMachine::new();
        assert_eq!(sm.current_state(), SessionState::Offline);
        assert_eq!(sm.previous_state(), None);
        assert_eq!(sm.last_reason(), None);
        assert!(!sm.is_transitory());
        assert!(!sm.is_timed_out());
    }

    #[test]
    fn test_valid_forward_lifecycle() {
        let mut sm = SessionStateMachine::new();

        // Offline -> Initializing
        sm.transition_to(SessionState::Initializing, Some("Bootstrapping daemon".into()))
            .expect("Offline -> Initializing should succeed");
        assert_eq!(sm.current_state(), SessionState::Initializing);
        assert_eq!(sm.previous_state(), Some(SessionState::Offline));

        // Initializing -> Ready
        sm.transition_to(SessionState::Ready, Some("PnP and IPC ready".into()))
            .expect("Initializing -> Ready should succeed");
        assert_eq!(sm.current_state(), SessionState::Ready);

        // Ready -> Pairing
        sm.transition_to(SessionState::Pairing, Some("Inbound peer pairing request".into()))
            .expect("Ready -> Pairing should succeed");
        assert_eq!(sm.current_state(), SessionState::Pairing);
        assert!(sm.is_transitory());
        assert_eq!(sm.timeout_duration(), Some(Duration::from_secs(30)));

        // Pairing -> Negotiating
        sm.transition_to(SessionState::Negotiating, Some("PIN verified".into()))
            .expect("Pairing -> Negotiating should succeed");
        assert_eq!(sm.current_state(), SessionState::Negotiating);
        assert!(sm.is_transitory());
        assert_eq!(sm.timeout_duration(), Some(Duration::from_secs(15)));

        // Negotiating -> Connecting
        sm.transition_to(SessionState::Connecting, Some("Cipher suite selected".into()))
            .expect("Negotiating -> Connecting should succeed");
        assert_eq!(sm.current_state(), SessionState::Connecting);
        assert!(sm.is_transitory());
        assert_eq!(sm.timeout_duration(), Some(Duration::from_secs(20)));

        // Connecting -> PreparingDevice
        sm.transition_to(SessionState::PreparingDevice, Some("Transport pipe open".into()))
            .expect("Connecting -> PreparingDevice should succeed");
        assert_eq!(sm.current_state(), SessionState::PreparingDevice);
        assert!(!sm.is_transitory());

        // PreparingDevice -> Active
        sm.transition_to(SessionState::Active, Some("USB filter attached and streaming".into()))
            .expect("PreparingDevice -> Active should succeed");
        assert_eq!(sm.current_state(), SessionState::Active);
        assert!(!sm.is_transitory());
    }

    #[test]
    fn test_fault_handling_and_recovery_cycle() {
        let mut sm = SessionStateMachine::with_initial_state(SessionState::Active);

        // Active -> Interrupted (Network dropped)
        sm.transition_to(SessionState::Interrupted, Some("Heartbeat lost".into()))
            .expect("Active -> Interrupted should succeed");
        assert_eq!(sm.current_state(), SessionState::Interrupted);

        // Interrupted -> Recovering
        sm.transition_to(SessionState::Recovering, Some("Attempting ICE restart".into()))
            .expect("Interrupted -> Recovering should succeed");
        assert_eq!(sm.current_state(), SessionState::Recovering);
        assert!(sm.is_transitory());
        assert_eq!(sm.timeout_duration(), Some(Duration::from_secs(10)));

        // Recovering -> Closing
        sm.transition_to(SessionState::Closing, Some("Recovery timed out, tearing down".into()))
            .expect("Recovering -> Closing should succeed");
        assert_eq!(sm.current_state(), SessionState::Closing);

        // Closing -> Ready
        sm.transition_to(SessionState::Ready, Some("Teardown clean, ready for new connection".into()))
            .expect("Closing -> Ready should succeed");
        assert_eq!(sm.current_state(), SessionState::Ready);
    }

    #[test]
    fn test_successful_recovery_to_active() {
        let mut sm = SessionStateMachine::with_initial_state(SessionState::Active);

        sm.transition_to(SessionState::Interrupted, Some("Socket glitch".into())).unwrap();
        sm.transition_to(SessionState::Recovering, Some("Reconnecting".into())).unwrap();
        sm.transition_to(SessionState::Active, Some("Reconnected successfully".into())).unwrap();
        assert_eq!(sm.current_state(), SessionState::Active);
    }

    #[test]
    fn test_degraded_transitions() {
        let mut sm = SessionStateMachine::with_initial_state(SessionState::Active);

        // Active -> Degraded
        sm.transition_to(SessionState::Degraded, Some("Packet loss > 5%".into())).unwrap();
        assert_eq!(sm.current_state(), SessionState::Degraded);

        // Degraded -> Active (Recovered link quality)
        sm.transition_to(SessionState::Active, Some("Packet loss normalized".into())).unwrap();
        assert_eq!(sm.current_state(), SessionState::Active);

        // Active -> Degraded -> Interrupted
        sm.transition_to(SessionState::Degraded, Some("Buffer stall".into())).unwrap();
        sm.transition_to(SessionState::Interrupted, Some("Link severed".into())).unwrap();
        assert_eq!(sm.current_state(), SessionState::Interrupted);
    }

    #[test]
    fn test_illegal_transitions_rejected() {
        let mut sm = SessionStateMachine::new();

        // Cannot jump directly from Offline to Active
        let res = sm.transition_to(SessionState::Active, None);
        assert!(res.is_err());
        assert_eq!(sm.current_state(), SessionState::Offline);

        // Cannot jump from Offline to Connecting
        let res = sm.transition_to(SessionState::Connecting, None);
        assert!(res.is_err());

        // Move to Ready
        sm.transition_to(SessionState::Initializing, None).unwrap();
        sm.transition_to(SessionState::Ready, None).unwrap();

        // Cannot jump from Ready directly to Active
        let res = sm.transition_to(SessionState::Active, None);
        assert!(res.is_err());

        // Cannot jump from Ready to Degraded
        let res = sm.transition_to(SessionState::Degraded, None);
        assert!(res.is_err());

        // Move to Active legally
        sm.transition_to(SessionState::Pairing, None).unwrap();
        sm.transition_to(SessionState::Negotiating, None).unwrap();
        sm.transition_to(SessionState::Connecting, None).unwrap();
        sm.transition_to(SessionState::PreparingDevice, None).unwrap();
        sm.transition_to(SessionState::Active, None).unwrap();

        // Cannot jump from Active back to Initializing
        let res = sm.transition_to(SessionState::Initializing, None);
        assert!(res.is_err());

        // Cannot jump from Active to Offline directly
        let res = sm.transition_to(SessionState::Offline, None);
        assert!(res.is_err());
    }

    #[test]
    fn test_transitory_timeouts_values() {
        assert_eq!(transitory_timeout(SessionState::Pairing), Some(Duration::from_secs(30)));
        assert_eq!(transitory_timeout(SessionState::Negotiating), Some(Duration::from_secs(15)));
        assert_eq!(transitory_timeout(SessionState::Connecting), Some(Duration::from_secs(20)));
        assert_eq!(transitory_timeout(SessionState::Recovering), Some(Duration::from_secs(10)));
        assert_eq!(transitory_timeout(SessionState::Active), None);
        assert_eq!(transitory_timeout(SessionState::Ready), None);
        assert_eq!(transitory_timeout(SessionState::Offline), None);
        assert_eq!(transitory_timeout(SessionState::PreparingDevice), None);
        assert_eq!(transitory_timeout(SessionState::Degraded), None);
        assert_eq!(transitory_timeout(SessionState::Interrupted), None);
        assert_eq!(transitory_timeout(SessionState::Closing), None);
        assert_eq!(transitory_timeout(SessionState::Failed), None);
    }

    #[test]
    fn test_timeout_expiration_logic() {
        let mut sm = SessionStateMachine::with_initial_state(SessionState::Pairing);
        assert!(!sm.is_timed_out());

        // Simulate 29 seconds elapsed -> not yet timed out
        sm.set_state_entered_at(Instant::now() - Duration::from_secs(29));
        assert!(!sm.is_timed_out());

        // Simulate 31 seconds elapsed -> timed out
        sm.set_state_entered_at(Instant::now() - Duration::from_secs(31));
        assert!(sm.is_timed_out());

        // Non-transitory state should never time out even after 1000s
        let mut sm_active = SessionStateMachine::with_initial_state(SessionState::Active);
        sm_active.set_state_entered_at(Instant::now() - Duration::from_secs(1000));
        assert!(!sm_active.is_timed_out());
    }
}
