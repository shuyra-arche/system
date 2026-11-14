use crate::{
    fixture::{self, Console},
    waiting::{Error, Interest, Waiting},
};
use core::fmt::Write as _;
use kernel_api::{
    abi::call::sign::SignReply,
    model::{identity::PendId, sign::SignSource, wait::Wait},
};
use kernel_client::call::{pend, sign};

pub fn run(license: PendId) {
    let baseline = fixture::active();
    let first = sign::create(license).unwrap();
    let second = sign::create(license).unwrap();
    let a = Interest::new(SignSource::Sign { token: first });
    let b = Interest::new(SignSource::Sign { token: second });
    let mut group = Waiting::new(license).unwrap();
    group.apply(&[], &[], &[a, a]).unwrap();
    assert_eq!(group.await_(Wait::Poll).unwrap(), SignReply::Pending);
    sign::trigger(first).unwrap();
    assert_eq!(
        group.await_(Wait::Poll).unwrap(),
        SignReply::Source {
            source: a.source,
            fail: None
        }
    );
    group.apply(&[], &[], &[b]).unwrap();
    assert_eq!(group.await_(Wait::Poll).unwrap(), SignReply::Pending);
    sign::trigger(second).unwrap();
    assert_eq!(
        group.await_(Wait::Poll).unwrap(),
        SignReply::Source {
            source: b.source,
            fail: None
        }
    );
    let invalid = Interest::new(SignSource::Sign {
        token: PendId::NONE,
    });
    assert!(matches!(
        group.apply(&[], &[], &[a, invalid]),
        Err(Error::Attach { .. })
    ));
    // The first attach succeeded before the later failure. Reconciliation must
    // retain and then detach it, rather than forget its live Kernel route.
    group.apply(&[], &[], &[b]).unwrap();
    assert_eq!(
        group.await_(Wait::Poll).unwrap(),
        SignReply::Source {
            source: b.source,
            fail: None
        }
    );
    let changed_generation = Interest {
        source: b.source,
        generation: 1,
    };
    assert!(matches!(
        group.apply(&[], &[], &[b, changed_generation]),
        Err(Error::Conflict)
    ));
    assert_eq!(
        group.await_(Wait::Poll).unwrap(),
        SignReply::Source {
            source: b.source,
            fail: None
        }
    );
    group.detach(PendId::NONE).unwrap();
    group.finish().unwrap();
    group.finish().unwrap();
    assert!(matches!(group.await_(Wait::Poll), Err(Error::Inactive)));
    pend::return_(first).unwrap();
    pend::return_(second).unwrap();
    assert_eq!(fixture::active(), baseline);
    writeln!(
        Console,
        "PASS wait-group dedup=1 replace=1 partial=1 conflict=1 cleanup=1"
    )
    .unwrap();
}
