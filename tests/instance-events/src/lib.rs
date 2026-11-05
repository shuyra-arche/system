#[cfg(test)]
#[path = "../../../server/src/system/assembly/observation.rs"]
mod observation;

#[cfg(test)]
mod system {
    pub mod app {
        pub mod events {
            pub fn emit(event: unit::event::Event<&str>) -> Result<(), &'static str> {
                if crate::FAIL.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    return Err("injected delivery failure");
                }
                crate::FRAMES
                    .lock()
                    .unwrap()
                    .push(unit::event::encode(&event).unwrap());
                Ok(())
            }
        }
    }
}

#[cfg(test)]
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(test)]
static FRAMES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use super::*;
    use unit::event::{Cause, Event, Exit};

    fn start() -> std::sync::MutexGuard<'static, ()> {
        let guard = SERIAL.lock().unwrap();
        FRAMES.lock().unwrap().clear();
        FAIL.store(false, std::sync::atomic::Ordering::SeqCst);
        guard
    }
    fn exit() -> Exit {
        Exit {
            cause: Cause::Reap,
            reason: 0,
        }
    }

    #[test]
    fn repeated_native_peeks_emit_one_exit_and_keep_the_native_cause() {
        let _guard = start();
        let mut state = observation::Observation::default();
        state.bind("shell", 13, 12).unwrap();
        state.ready(13).unwrap();
        state.ready(13).unwrap();
        let native = Exit {
            cause: Cause::Fault,
            reason: 23,
        };
        state.exit(13, native).unwrap();
        state.exit(13, native).unwrap();
        state.retired(13).unwrap();
        let frames = FRAMES.lock().unwrap();
        let events: Vec<Event<String>> = frames
            .iter()
            .map(|frame| unit::event::decode(frame.as_bytes()).unwrap())
            .collect();
        assert_eq!(events.len(), 4);
        assert!(matches!(&events[0], Event::Bound { unit, task:13, team:12 } if unit=="shell"));
        assert!(matches!(events[1], Event::Ready { task: 13 }));
        assert!(matches!(
            events[2],
            Event::Exited {
                task: 13,
                exit: Exit {
                    cause: Cause::Fault,
                    reason: 23
                }
            }
        ));
        assert!(matches!(events[3], Event::Retired { task: 13 }));
    }

    #[test]
    fn retirement_and_readiness_cannot_be_invented_from_control_state() {
        let _guard = start();
        let mut state = observation::Observation::default();
        assert!(state.ready(13).is_err());
        assert!(state.exit(13, exit()).is_err());
        assert!(state.retired(13).is_err());
        state.bind("shell", 13, 12).unwrap();
        assert!(state.retired(13).is_err());
        state.exit(13, exit()).unwrap();
        assert!(state.ready(13).is_err());
        state.retired(13).unwrap();
        assert_eq!(FRAMES.lock().unwrap().len(), 3);
    }

    #[test]
    fn a_failed_delivery_does_not_advance_publication_state() {
        let _guard = start();
        let mut state = observation::Observation::default();
        FAIL.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(state.bind("shell", 13, 12).is_err());
        state.bind("shell", 13, 12).unwrap();
        FAIL.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(state.exit(13, exit()).is_err());
        assert!(state.retired(13).is_err());
        state.exit(13, exit()).unwrap();
        state.retired(13).unwrap();
        assert_eq!(FRAMES.lock().unwrap().len(), 3);
    }
}
