use kernel_api::{
    abi::{
        call::pend::PendFail,
        memory::{Call, Domain, Failure, RequestId},
    },
    model::{identity::PendId, nest::NestId, pend::PendInfo},
};
use kernel_client::memory::{self, Response};
use std::{cell::Cell, rc::Rc};
use system_client::{CleanupError, Context};
use twine::view::{
    clone::{ReceiveError, ReplyError},
    memory::{MappingError, Memory, Receipt},
};

struct Requests {
    ids: std::array::IntoIter<u64, 2>,
    dropped: Rc<Cell<bool>>,
}
impl Drop for Requests {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}
// SAFETY: only request allocation is exercised. All authority/effect methods
// panic; this provider can never fabricate a successful mapping or a Pend.
unsafe impl Memory for Requests {
    fn space(&self) -> (PendId, NestId) {
        panic!("no host Nest")
    }
    fn next_request(&mut self) -> Option<RequestId> {
        self.ids.next().and_then(RequestId::new)
    }
    fn invoke(&mut self, _: Call) -> Result<Response, memory::Error> {
        panic!("no host effects")
    }
    fn inspect_pend(&mut self, _: PendId) -> Result<PendInfo, erra::Error<PendFail>> {
        panic!("no host Pend")
    }
    fn return_pend(&mut self, _: PendId) -> Result<(), erra::Error<PendFail>> {
        panic!("no host Pend")
    }
}

#[test]
fn retained_contexts_share_one_sequence_and_preserve_exhaustion() {
    let dropped = Rc::new(Cell::new(false));
    let mut client = Context::new(Requests {
        ids: [1, u64::MAX].into_iter(),
        dropped: dropped.clone(),
    });
    let mut pending = client.clone();
    assert_eq!(client.next_request().unwrap().get(), 1);
    drop(client);
    assert!(
        !dropped.get(),
        "a retained response still owns its allocator"
    );
    assert_eq!(pending.next_request().unwrap().get(), u64::MAX);
    assert!(pending.next_request().is_none());
    let mut cleanup = pending.clone();
    drop(pending);
    assert!(
        cleanup.next_request().is_none(),
        "cleanup must not reset exhaustion"
    );
    drop(cleanup);
    assert!(dropped.get());
}

fn cleanup(error: MappingError) -> CleanupError {
    CleanupError::Reply(ReplyError::Receive(ReceiveError::Mapping(error)))
}
#[test]
fn mapping_cleanup_distinguishes_retryable_progress_from_retained_failure() {
    let receipt = Receipt {
        request: RequestId::new(1).unwrap(),
        domain: Domain::Nest,
    };
    assert!(cleanup(MappingError::Pending { receipt }).is_busy());
    assert!(cleanup(MappingError::Rejected(Failure::Busy)).is_busy());
    assert!(
        cleanup(MappingError::Uncertain {
            receipt,
            error: memory::Error::Delivery(Failure::Busy)
        })
        .is_busy()
    );
    assert!(
        !cleanup(MappingError::Failed {
            receipt,
            failure: Failure::Hardware,
            retained: None
        })
        .is_busy()
    );
    assert!(!cleanup(MappingError::Expired { receipt }).is_busy());
}
