//! The presence state machine — the single source of truth that drives
//! the orb's color + motion. This is the heart of the client.
//!
//! States are the *presence* of Skye; `muted` is an orthogonal flag, not a
//! state, because you can be muted in any state.

use serde::{Deserialize, Serialize};

/// The presence state of the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Disconnected,
    Idle,
    Listening,
    Thinking,
    Speaking,
}

impl State {
    /// Orb color for this state (CSS color). Motion first, color second,
    /// label third — colorblind-safe.
    pub fn color(&self) -> &'static str {
        match self {
            State::Disconnected => "#4a4a55", // grey, dim
            State::Idle => "#5b8def",         // soft blue (Skye's identity)
            State::Listening => "#3b82f6",    // bright blue
            State::Thinking => "#f59e0b",     // amber
            State::Speaking => "#14b8a6",     // teal
        }
    }

    /// Orb motion for this state.
    pub fn motion(&self) -> &'static str {
        match self {
            State::Disconnected => "static",
            State::Idle => "breathing",
            State::Listening => "level",
            State::Thinking => "pulsing",
            State::Speaking => "level",
        }
    }
}

/// The full client state: presence state + the orthogonal muted flag.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ClientState {
    pub state: State,
    pub muted: bool,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            state: State::Disconnected,
            muted: false,
        }
    }
}

/// A transition request. The machine decides whether it is legal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Connect,
    Disconnect,
    WakeWord,
    UtteranceComplete,
    ResponseStarted,
    ResponseComplete,
    ToggleMute,
    SetMute(bool),
}

/// The state machine. Owns the current state and validates transitions.
pub struct StateMachine {
    current: ClientState,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            current: ClientState::default(),
        }
    }

    pub fn current(&self) -> ClientState {
        self.current
    }

    /// Apply a transition. Returns `Some(new_state)` if the transition
    /// changed state, `None` if it was a no-op or illegal.
    pub fn apply(&mut self, t: Transition) -> Option<ClientState> {
        let next = match t {
            Transition::Connect => {
                if self.current.state == State::Disconnected {
                    State::Idle
                } else {
                    return None;
                }
            }
            Transition::Disconnect => {
                if self.current.state != State::Disconnected {
                    State::Disconnected
                } else {
                    return None;
                }
            }
            Transition::WakeWord => {
                // Cannot listen while muted or disconnected.
                if self.current.state == State::Idle && !self.current.muted {
                    State::Listening
                } else {
                    return None;
                }
            }
            Transition::UtteranceComplete => {
                if self.current.state == State::Listening {
                    State::Thinking
                } else {
                    return None;
                }
            }
            Transition::ResponseStarted => {
                if self.current.state == State::Thinking {
                    State::Speaking
                } else {
                    return None;
                }
            }
            Transition::ResponseComplete => {
                if self.current.state == State::Speaking {
                    State::Idle
                } else {
                    return None;
                }
            }
            Transition::ToggleMute => {
                self.current.muted = !self.current.muted;
                return Some(self.current);
            }
            Transition::SetMute(m) => {
                if self.current.muted != m {
                    self.current.muted = m;
                    return Some(self.current);
                }
                return None;
            }
        };

        self.current.state = next;
        Some(self.current)
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_disconnected() {
        let m = StateMachine::new();
        assert_eq!(m.current().state, State::Disconnected);
        assert!(!m.current().muted);
    }

    #[test]
    fn full_happy_path() {
        let mut m = StateMachine::new();
        assert!(m.apply(Transition::Connect).is_some());
        assert_eq!(m.current().state, State::Idle);

        assert!(m.apply(Transition::WakeWord).is_some());
        assert_eq!(m.current().state, State::Listening);

        assert!(m.apply(Transition::UtteranceComplete).is_some());
        assert_eq!(m.current().state, State::Thinking);

        assert!(m.apply(Transition::ResponseStarted).is_some());
        assert_eq!(m.current().state, State::Speaking);

        assert!(m.apply(Transition::ResponseComplete).is_some());
        assert_eq!(m.current().state, State::Idle);
    }

    #[test]
    fn cannot_listen_while_muted() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        m.apply(Transition::SetMute(true));
        assert!(m.apply(Transition::WakeWord).is_none());
        assert_eq!(m.current().state, State::Idle);
    }

    #[test]
    fn illegal_transitions_are_noops() {
        let mut m = StateMachine::new();
        // Cannot wake word while disconnected.
        assert!(m.apply(Transition::WakeWord).is_none());
        // Cannot complete utterance while not listening.
        assert!(m.apply(Transition::UtteranceComplete).is_none());
        // Double connect is a no-op.
        m.apply(Transition::Connect);
        assert!(m.apply(Transition::Connect).is_none());
    }

    #[test]
    fn disconnect_from_any_state() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        m.apply(Transition::WakeWord);
        assert!(m.apply(Transition::Disconnect).is_some());
        assert_eq!(m.current().state, State::Disconnected);
    }
}
