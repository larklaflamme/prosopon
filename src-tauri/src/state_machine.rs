//! The presence state machine — the single source of truth that drives
//! the orb's color + motion. This is the heart of the client.
//!
//! States are the *presence* of Skye; `muted` and `conversation_active` are
//! orthogonal flags, not states, because you can be muted (or in a warm
//! multi-turn conversation) in any state.

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

/// The full client state: presence state + the orthogonal flags.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ClientState {
    pub state: State,
    pub muted: bool,
    /// True while in a warm multi-turn conversation (no wake word needed
    /// between turns). False when cold (wake word required to listen).
    pub conversation_active: bool,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            state: State::Disconnected,
            muted: false,
            conversation_active: false,
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
    /// Interrupt in-flight speech and hand the floor back to the user.
    /// Legal from Speaking while warm: Speaking → Listening.
    BargeIn,
    /// The warm conversation's inactivity gap elapsed. Ends the session:
    /// Listening → Idle, conversation_active = false.
    InactivityTimeout,
    /// Abort an in-flight listen/think and return to idle (e.g. silence
    /// timeout, or a send/receive error). Legal from Listening or Thinking.
    Cancel,
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
                    self.current.conversation_active = false;
                    State::Disconnected
                } else {
                    return None;
                }
            }
            Transition::WakeWord => {
                // Cannot listen while muted or disconnected. Only from Idle
                // (cold) — in a warm conversation the user just speaks.
                if self.current.state == State::Idle && !self.current.muted {
                    self.current.conversation_active = true;
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
                    // Warm conversation → keep listening; cold → back to idle.
                    if self.current.conversation_active {
                        State::Listening
                    } else {
                        State::Idle
                    }
                } else {
                    return None;
                }
            }
            Transition::BargeIn => {
                // Interrupt speech, hand the floor back. Only while warm.
                if self.current.state == State::Speaking && self.current.conversation_active {
                    State::Listening
                } else {
                    return None;
                }
            }
            Transition::InactivityTimeout => {
                // End the warm session after the silence gap.
                if self.current.state == State::Listening {
                    self.current.conversation_active = false;
                    State::Idle
                } else {
                    return None;
                }
            }
            Transition::Cancel => {
                match self.current.state {
                    State::Listening | State::Thinking => {
                        self.current.conversation_active = false;
                        State::Idle
                    }
                    _ => return None,
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
        assert!(!m.current().conversation_active);
    }

    #[test]
    fn full_happy_path_warm() {
        let mut m = StateMachine::new();
        assert!(m.apply(Transition::Connect).is_some());
        assert_eq!(m.current().state, State::Idle);

        assert!(m.apply(Transition::WakeWord).is_some());
        assert_eq!(m.current().state, State::Listening);
        assert!(m.current().conversation_active);

        assert!(m.apply(Transition::UtteranceComplete).is_some());
        assert_eq!(m.current().state, State::Thinking);

        assert!(m.apply(Transition::ResponseStarted).is_some());
        assert_eq!(m.current().state, State::Speaking);

        // Warm: response completes back into Listening, not Idle.
        assert!(m.apply(Transition::ResponseComplete).is_some());
        assert_eq!(m.current().state, State::Listening);
        assert!(m.current().conversation_active);

        // Second turn without a wake word.
        assert!(m.apply(Transition::UtteranceComplete).is_some());
        assert_eq!(m.current().state, State::Thinking);
        assert!(m.apply(Transition::ResponseStarted).is_some());
        assert!(m.apply(Transition::ResponseComplete).is_some());
        assert_eq!(m.current().state, State::Listening);

        // Inactivity gap ends the warm session.
        assert!(m.apply(Transition::InactivityTimeout).is_some());
        assert_eq!(m.current().state, State::Idle);
        assert!(!m.current().conversation_active);
    }

    #[test]
    fn barge_in_interrupts_speech() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        m.apply(Transition::WakeWord);
        m.apply(Transition::UtteranceComplete);
        m.apply(Transition::ResponseStarted);
        assert_eq!(m.current().state, State::Speaking);

        assert!(m.apply(Transition::BargeIn).is_some());
        assert_eq!(m.current().state, State::Listening);
        assert!(m.current().conversation_active);
    }

    #[test]
    fn barge_in_illegal_when_cold() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        // Cold (never woke): BargeIn from Speaking is impossible anyway, but
        // guard the flag path: a cold machine can't be Speaking without a
        // wake word, so this is just a no-op sanity check.
        assert!(m.apply(Transition::BargeIn).is_none());
    }

    #[test]
    fn inactivity_timeout_ends_warm_session() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        m.apply(Transition::WakeWord);
        assert_eq!(m.current().state, State::Listening);
        assert!(m.current().conversation_active);

        assert!(m.apply(Transition::InactivityTimeout).is_some());
        assert_eq!(m.current().state, State::Idle);
        assert!(!m.current().conversation_active);

        // After going cold, the wake word is required again.
        assert!(m.apply(Transition::WakeWord).is_some());
        assert_eq!(m.current().state, State::Listening);
    }

    #[test]
    fn cancel_from_listening_returns_to_idle_and_cools() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        m.apply(Transition::WakeWord);
        assert_eq!(m.current().state, State::Listening);
        assert!(m.apply(Transition::Cancel).is_some());
        assert_eq!(m.current().state, State::Idle);
        assert!(!m.current().conversation_active);
    }

    #[test]
    fn cancel_from_thinking_returns_to_idle() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        m.apply(Transition::WakeWord);
        m.apply(Transition::UtteranceComplete);
        assert_eq!(m.current().state, State::Thinking);
        assert!(m.apply(Transition::Cancel).is_some());
        assert_eq!(m.current().state, State::Idle);
    }

    #[test]
    fn cancel_is_illegal_from_idle() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        assert!(m.apply(Transition::Cancel).is_none());
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
        // Cannot complete utterance while idle.
        assert!(m.apply(Transition::UtteranceComplete).is_none());
        // Cannot start response while idle.
        assert!(m.apply(Transition::ResponseStarted).is_none());
        // Cannot complete response while idle.
        assert!(m.apply(Transition::ResponseComplete).is_none());
        // Cannot barge in while idle.
        assert!(m.apply(Transition::BargeIn).is_none());
        // Cannot time out while idle.
        assert!(m.apply(Transition::InactivityTimeout).is_none());
    }

    #[test]
    fn disconnect_resets_conversation() {
        let mut m = StateMachine::new();
        m.apply(Transition::Connect);
        m.apply(Transition::WakeWord);
        assert!(m.current().conversation_active);
        assert!(m.apply(Transition::Disconnect).is_some());
        assert_eq!(m.current().state, State::Disconnected);
        assert!(!m.current().conversation_active);
    }
}
