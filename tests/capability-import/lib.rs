#![allow(dead_code, unused_imports)]
extern crate alloc;
extern crate self as programs;
extern crate self as resource;
use env::{Mark, PieToken, TaskId};
use std::cell::RefCell;
#[derive(Clone, Copy)]
struct Native {
    token: PieToken,
    giver: TaskId,
    owner: TaskId,
    mark: Mark,
    live: bool,
    hole: bool,
}
thread_local! {
    static ENTRIES: RefCell<Vec<Native>> = const { RefCell::new(Vec::new()) };
    static SENT: RefCell<Vec<PieToken>> = const { RefCell::new(Vec::new()) };
}
pub mod debug {
    pub fn put(_: &str) {}
}
pub mod raw {
    use super::*;
    pub fn alive(token: PieToken) -> bool {
        ENTRIES.with(|es| es.borrow().iter().any(|e| e.token == token && e.live))
    }
    pub fn reserve(token: PieToken) -> Result<(TaskId, TaskId, Mark), ()> {
        ENTRIES.with(|es| {
            es.borrow()
                .iter()
                .find(|e| e.token == token && e.hole && e.live)
                .map(|e| (e.giver, e.owner, e.mark))
                .ok_or(())
        })
    }
    pub struct Hole(PieToken);
    pub struct Fail {
        pub source: Source,
    }
    pub struct Source;
    impl Source {
        pub fn is_busy(&self) -> bool {
            false
        }
    }
    impl Hole {
        pub fn from_raw(token: PieToken) -> Self {
            Self(token)
        }
        pub fn push(&self, _: &[u8], _: env::Wait) -> Result<(), Fail> {
            if !alive(self.0) {
                return Err(Fail { source: Source });
            }
            SENT.with(|s| s.borrow_mut().push(self.0));
            Ok(())
        }
    }
}
#[path = "../../server/src/system/operator/watch.rs"]
mod watch;
#[cfg(test)]
mod tests {
    use super::*;
    use system_api::operator::path::Path;
    use system_api::operator::{EntryId, Event, Kind, WATCH_MARK};
    fn who() -> TaskId {
        TaskId::new(7)
    }
    fn native(mark: Mark) -> Native {
        Native {
            token: PieToken::mint(10),
            giver: who(),
            owner: who(),
            mark,
            live: true,
            hole: true,
        }
    }
    fn set(entry: Native) {
        ENTRIES.with(|e| *e.borrow_mut() = vec![entry]);
    }
    fn event() -> Event {
        Event {
            seq: 0,
            kind: Kind::Landed,
            road: Path::new("svc/a").to_path_buf(),
            id: EntryId::new(1),
            owner: who(),
        }
    }
    #[test]
    fn watch_rejects_forged_imports_without_mutating_existing_subscription() {
        let valid = native(WATCH_MARK);
        set(valid);
        let mut watchers = watch::Watchers::new();
        watchers
            .join(
                watch::Subscription::import(who(), Path::new("svc").to_path_buf(), valid.token)
                    .unwrap(),
            )
            .unwrap();
        let cases = [
            Native {
                giver: TaskId::new(8),
                ..valid
            },
            Native {
                owner: TaskId::new(8),
                ..valid
            },
            Native {
                mark: Mark::NONE,
                ..valid
            },
            Native {
                live: false,
                ..valid
            },
            Native {
                hole: false,
                ..valid
            },
        ];
        for bad in cases {
            set(bad);
            assert!(
                watch::Subscription::import(who(), Path::ROOT.to_path_buf(), bad.token).is_err()
            );
            assert_eq!(watchers.len(), 1);
            assert_eq!(ENTRIES.with(|es| es.borrow().len()), 1);
        }
        ENTRIES.with(|es| es.borrow_mut().clear());
        assert!(watch::Subscription::import(who(), Path::ROOT.to_path_buf(), valid.token).is_err());
        set(valid);
        assert_eq!(watchers.publish(event()), 1);
        assert_eq!(SENT.with(|s| s.borrow().clone()), vec![valid.token]);
    }
    #[test]
    fn watch_retires_only_dead_subscription_and_keeps_other_tasks() {
        let a = native(WATCH_MARK);
        let b = Native {
            token: PieToken::mint(11),
            giver: TaskId::new(8),
            owner: TaskId::new(8),
            ..a
        };
        ENTRIES.with(|es| *es.borrow_mut() = vec![a, b]);
        let mut watchers = watch::Watchers::new();
        for entry in [a, b] {
            watchers
                .join(
                    watch::Subscription::import(entry.owner, Path::ROOT.to_path_buf(), entry.token)
                        .unwrap(),
                )
                .unwrap();
        }
        ENTRIES.with(|es| es.borrow_mut()[0].live = false);
        assert_eq!(watchers.publish(event()), 1);
        assert_eq!(watchers.len(), 1);
        assert_eq!(SENT.with(|s| s.borrow().clone()), vec![b.token]);
    }
    #[test]
    fn closed_watch_can_rejoin_same_task_and_path_without_waiting_for_an_event() {
        let old = native(WATCH_MARK);
        let replacement = Native {
            token: PieToken::mint(12),
            ..old
        };
        ENTRIES.with(|entries| *entries.borrow_mut() = vec![old, replacement]);
        let mut watchers = watch::Watchers::new();
        watchers
            .join(watch::Subscription::import(who(), Path::ROOT.to_path_buf(), old.token).unwrap())
            .unwrap();
        ENTRIES.with(|entries| entries.borrow_mut()[0].live = false);
        watchers
            .join(
                watch::Subscription::import(who(), Path::ROOT.to_path_buf(), replacement.token)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(watchers.len(), 1);
        SENT.with(|sent| sent.borrow_mut().clear());
        assert_eq!(watchers.publish(event()), 1);
        assert_eq!(
            SENT.with(|sent| sent.borrow().clone()),
            vec![replacement.token]
        );
    }
}
