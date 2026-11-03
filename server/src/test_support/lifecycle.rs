use env::unit;

pub fn until(mut done: impl FnMut() -> bool) {
    let deadline = env::chrono::clock() + 2_000_000_000;
    while !done() {
        assert!(env::chrono::clock() < deadline, "lifecycle: timed out");
        execution::room::park(core::time::Duration::from_millis(1)).unwrap();
    }
}

pub fn oust(team: env::TeamId) {
    until(|| match unit::oust(team) {
        Ok(()) => true,
        Err(e) if e.source == env::UnitFail::Busy => false,
        Err(e) => panic!("lifecycle: oust {team:?}: {e:?}"),
    });
}
