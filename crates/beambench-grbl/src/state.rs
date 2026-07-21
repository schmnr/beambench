//! Session state machine with validated transitions.

use crate::error::GrblError;
use beambench_common::machine::SessionState;

/// State machine that validates session state transitions.
#[derive(Debug)]
pub struct SessionStateMachine {
    state: SessionState,
}

impl SessionStateMachine {
    pub fn new() -> Self {
        Self {
            state: SessionState::Disconnected,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Attempt to transition to a new state. Returns an error if the transition is invalid.
    pub fn transition(&mut self, to: SessionState) -> Result<(), GrblError> {
        if self.state == to {
            return Ok(());
        }

        if self.is_valid_transition(to) {
            self.state = to;
            Ok(())
        } else {
            Err(GrblError::InvalidTransition {
                from: format!("{:?}", self.state),
                to: format!("{to:?}"),
            })
        }
    }

    /// Force state (for error recovery).
    pub fn force(&mut self, state: SessionState) {
        self.state = state;
    }

    fn is_valid_transition(&self, to: SessionState) -> bool {
        use SessionState::*;
        matches!(
            (self.state, to),
            // Normal connect flow
            (Disconnected, Connecting)
                | (Connecting, TransportOpen)
                | (Connecting, Error)
                | (Connecting, Disconnected)
                | (TransportOpen, WaitingForBanner)
                | (TransportOpen, Error)
                | (WaitingForBanner, Validating)
                | (WaitingForBanner, Error)
                | (Validating, Ready)
                | (Validating, Alarm)
                | (Validating, Error)
                // Ready state transitions
                | (Ready, Running)
                | (Ready, Alarm)
                | (Ready, Error)
                | (Ready, Disconnected)
                // Running state transitions
                | (Running, Paused)
                | (Running, Ready)
                | (Running, Alarm)
                | (Running, Error)
                // Paused state transitions
                | (Paused, Running)
                | (Paused, Ready)
                | (Paused, Alarm)
                | (Paused, Error)
                | (Paused, Disconnected)
                // Alarm recovery
                | (Alarm, Ready)
                | (Alarm, Disconnected)
                | (Alarm, Error)
                // Error recovery
                | (Error, Disconnected)
                | (Error, Connecting)
        )
    }
}

impl Default for SessionStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::machine::SessionState;

    #[test]
    fn starts_disconnected() {
        let sm = SessionStateMachine::new();
        assert_eq!(sm.state(), SessionState::Disconnected);
    }

    #[test]
    fn valid_connect_flow() {
        let mut sm = SessionStateMachine::new();
        sm.transition(SessionState::Connecting).unwrap();
        sm.transition(SessionState::TransportOpen).unwrap();
        sm.transition(SessionState::WaitingForBanner).unwrap();
        sm.transition(SessionState::Validating).unwrap();
        sm.transition(SessionState::Ready).unwrap();
        assert_eq!(sm.state(), SessionState::Ready);
    }

    #[test]
    fn invalid_transition_rejected() {
        let mut sm = SessionStateMachine::new();
        assert!(sm.transition(SessionState::Ready).is_err());
    }

    #[test]
    fn same_state_transitions_are_noops() {
        for state in [
            SessionState::Disconnected,
            SessionState::Connecting,
            SessionState::TransportOpen,
            SessionState::WaitingForBanner,
            SessionState::Validating,
            SessionState::Ready,
            SessionState::Running,
            SessionState::Paused,
            SessionState::Alarm,
            SessionState::Error,
        ] {
            let mut sm = SessionStateMachine::new();
            sm.force(state);

            sm.transition(state).unwrap();

            assert_eq!(sm.state(), state);
        }
    }

    #[test]
    fn ready_to_running() {
        let mut sm = SessionStateMachine::new();
        sm.force(SessionState::Ready);
        sm.transition(SessionState::Running).unwrap();
        assert_eq!(sm.state(), SessionState::Running);
    }

    #[test]
    fn running_to_paused_and_back() {
        let mut sm = SessionStateMachine::new();
        sm.force(SessionState::Running);
        sm.transition(SessionState::Paused).unwrap();
        sm.transition(SessionState::Running).unwrap();
        assert_eq!(sm.state(), SessionState::Running);
    }

    #[test]
    fn alarm_recovery() {
        let mut sm = SessionStateMachine::new();
        sm.force(SessionState::Alarm);
        sm.transition(SessionState::Ready).unwrap();
        assert_eq!(sm.state(), SessionState::Ready);
    }

    #[test]
    fn error_to_disconnected() {
        let mut sm = SessionStateMachine::new();
        sm.force(SessionState::Error);
        sm.transition(SessionState::Disconnected).unwrap();
        assert_eq!(sm.state(), SessionState::Disconnected);
    }

    #[test]
    fn force_overrides_state() {
        let mut sm = SessionStateMachine::new();
        sm.force(SessionState::Running);
        assert_eq!(sm.state(), SessionState::Running);
    }

    #[test]
    fn connecting_to_error() {
        let mut sm = SessionStateMachine::new();
        sm.transition(SessionState::Connecting).unwrap();
        sm.transition(SessionState::Error).unwrap();
        assert_eq!(sm.state(), SessionState::Error);
    }
}
