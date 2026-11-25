//! UI-agnostic change notifications from the core.
//! The core publishes facts; the shell forwards them to whatever UI it has.

use serde::Serialize;
use tokio::sync::broadcast;

/// One change of core state. Serialized with a `kind` tag, so the UI can
/// switch on `payload.kind`.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// The game process set changed (a launch began, a process appeared or
    /// ended). Lists mirror `game.running`.
    GameRunning {
        running: Vec<String>,
        starting: Vec<String>,
    },
    /// One session ended (natural exit, stop or crash). Sent before the
    /// matching `game_running`, so the UI has the exit when the list shrinks.
    GameExit {
        instance: String,
        exit: crate::game::running::Exit,
    },
    /// Progress for one key — the same key `game.progress` accepts (instance
    /// id, import token, `java:<major>`, `move:instances`). Stage changes are
    /// sent at once; counter updates at most every 250 ms.
    GameProgress {
        key: String,
        progress: serde_json::Value,
    },
}

/// The core's event bus. Delivery is best-effort: publishing with no
/// subscribers is a no-op, a slow reader gets `Lagged` and resyncs.
pub struct Events {
    tx: broadcast::Sender<Event>, // capacity 64
}

impl Default for Events {
    fn default() -> Self {
        Self {
            tx: broadcast::channel(64).0,
        }
    }
}

impl Events {
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Delivery is best-effort: no subscribers or a closed channel is normal.
    pub fn send(&self, ev: Event) {
        let _ = self.tx.send(ev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn exit(code: i32) -> crate::game::running::Exit {
        crate::game::running::Exit {
            code: Some(code),
            stopped: false,
            secs: 1,
            at: 0,
            crashed: code != 0,
        }
    }

    /// Publishing with nobody listening is normal (tests, headless cores).
    #[test]
    fn send_without_subscribers_is_a_no_op() {
        let events = Events::default();
        events.send(Event::GameRunning {
            running: vec![],
            starting: vec![],
        });
        // Subscribing after the fact still gets the following events.
        let mut rx = events.subscribe();
        events.send(Event::GameRunning {
            running: vec!["a".into()],
            starting: vec![],
        });
        assert!(matches!(
            rx.blocking_recv().unwrap(),
            Event::GameRunning { .. }
        ));
    }

    /// Every subscriber sees its own copy.
    #[test]
    fn every_subscriber_sees_the_event() {
        let events = Events::default();
        let mut a = events.subscribe();
        let mut b = events.subscribe();
        events.send(Event::GameExit {
            instance: "i".into(),
            exit: exit(0),
        });
        for rx in [&mut a, &mut b] {
            match rx.blocking_recv().unwrap() {
                Event::GameExit { instance, exit } => {
                    assert_eq!(instance, "i");
                    assert_eq!(exit.code, Some(0));
                }
                ev => panic!("unexpected event: {ev:?}"),
            }
        }
    }

    /// A slow reader is told it lagged and then keeps receiving new events.
    #[test]
    fn lagged_subscriber_resyncs_and_continues() {
        let events = Events::default();
        let mut rx = events.subscribe();
        for i in 0..70 {
            events.send(Event::GameProgress {
                key: i.to_string(),
                progress: json!({}),
            });
        }
        assert!(matches!(
            rx.blocking_recv(),
            Err(broadcast::error::RecvError::Lagged(_))
        ));
        // The ring still holds the newest events: drain them without blocking
        // (a send here would push the next expected one out and lag again).
        let mut drained = 0;
        while rx.try_recv().is_ok() {
            drained += 1;
        }
        assert!(drained > 0, "nothing retained after the lag");
        events.send(Event::GameProgress {
            key: "after".into(),
            progress: json!({}),
        });
        match rx.blocking_recv().unwrap() {
            Event::GameProgress { key, .. } => assert_eq!(key, "after"),
            ev => panic!("unexpected event: {ev:?}"),
        }
    }

    /// The wire format the UI switches on: a `kind` tag per event.
    #[test]
    fn events_carry_a_kind_tag() {
        let v = serde_json::to_value(Event::GameProgress {
            key: "k".into(),
            progress: json!({ "done_files": 1 }),
        })
        .unwrap();
        assert_eq!(v["kind"], "game_progress");
        let v = serde_json::to_value(Event::GameRunning {
            running: vec![],
            starting: vec![],
        })
        .unwrap();
        assert_eq!(v["kind"], "game_running");
        let v = serde_json::to_value(Event::GameExit {
            instance: "i".into(),
            exit: exit(0),
        })
        .unwrap();
        assert_eq!(v["kind"], "game_exit");
        assert_eq!(v["exit"]["crashed"], false);
    }
}
