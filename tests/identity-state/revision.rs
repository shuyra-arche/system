use super::*;

#[test]
fn revision_exhaustion_never_wraps_or_republishes_an_old_cache_version() {
    let epoch = Epoch(Arc::new(AtomicU64::new(u64::MAX - 1)));
    let mut publisher = Publisher::new(epoch.clone());
    publisher.record().unwrap();
    assert_eq!(epoch.current(), u64::MAX);
    assert!(matches!(publisher.record(), Err(Error::Exhausted)));
    assert_eq!(epoch.current(), u64::MAX);
}
