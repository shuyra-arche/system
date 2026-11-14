//! Exercise actual cross-Team grants in both System worker/Unit directions.
use crate::fixture::{self, Console, addr, until};
use crate::loader_memory::Mapping;
use core::{
    fmt::Write as _,
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use kernel_api::{
    abi::call::unit::UnitFail,
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{Allocation, CellObject, CellPosition, Placement},
        identity::{PendId, TaskId},
        modifiable::Modifiable,
        pend::{CreationRights, NarrowRequest, Operations, PendInfo},
        unit::{ControlId, CreatedUnit, UnitCreate, UnitQuery, UnitTarget},
    },
};
use kernel_client::call::{cell, pend, room, share_view, unit};
use twine::view::clone::{ReceiveError, Reply, ReplyError, Sender};
use weave::FrameReceiver;

unsafe extern "C" {
    static __code_start: u8;
    static __code_end: u8;
}
// The System sibling shares these ordinary synchronization words. The foreign
// User Task receives only RX code and never reads these writable globals.
static FOREIGN: AtomicUsize = AtomicUsize::new(0);
static PROBED: AtomicBool = AtomicBool::new(false);
static FINISH: AtomicBool = AtomicBool::new(false);

pub fn run(license: PendId, creator: TaskId) {
    let (sibling_target, sibling) = fixture::spawn(worker);
    let mut local = Reply::create(license, 64).unwrap();
    local.grant(sibling).unwrap();
    local.finish().unwrap();

    // Copy only immutable code. The foreign task uses its own Kernel stack and
    // no allocator or mutable globals from the bootstrap Team.
    let start = ptr::addr_of!(__code_start);
    let len = ptr::addr_of!(__code_end).addr() - start.addr();
    let pages = len.div_ceil(PAGE_SIZE);
    let slot = cell::create(
        license,
        Allocation::new(pages * PAGE_SIZE, PAGE_SIZE).unwrap(),
    )
    .unwrap();
    let source = share_view::create(license, slot, Modifiable::Yes).unwrap();
    let (destination, span) = share_view::open(source).unwrap();
    assert!(span >= len);
    // SAFETY: The fresh Slot mapping is disjoint and writable, source is RX.
    unsafe {
        ptr::copy_nonoverlapping(start, destination.get() as *mut u8, len);
    }
    share_view::shut(source).unwrap();
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(source, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap();
    let mut limits = PendInfo::from_bytes(&bytes).unwrap().limits;
    limits.modifiable = Modifiable::No;
    pend::narrow(addr(
        &NarrowRequest {
            token: source,
            limits,
        }
        .to_bytes(),
    ))
    .unwrap();
    let create = UnitCreate::Team {
        member_exit: false,
        nest: None,
        guard: ControlId::NONE,
    };
    let CreatedUnit::Team { team, nest: whole } = unit::create(addr(&create.to_bytes())).unwrap()
    else {
        panic!("Team expected")
    };
    let mut mapping = Mapping::new(
        CellPosition {
            object: CellObject::Pend(source),
            page: 0,
            generation: 0,
        },
        pages,
        whole,
        Placement::exact(VirtAddr::new(start.addr()), PAGE_SIZE).unwrap(),
        10,
    )
    .unwrap();
    until(|| mapping.advance().unwrap());
    let arguments = [creator.get(), sibling.get()];
    let create = UnitCreate::Task {
        team: Some(team),
        entry: VirtAddr::new(foreign as *const () as usize),
        args: VirtAddr::new(arguments.as_ptr().addr()),
        count: 2,
        stack: VirtAddr::new(0),
    };
    let created = unit::create(addr(&create.to_bytes())).unwrap();
    let CreatedUnit::Task(task) = created else {
        unreachable!()
    };
    let target = UnitTarget::Task(task);
    fixture::grant(
        license,
        task,
        Operations::None(CreationRights::CLONE),
        Modifiable::No,
    );
    fixture::grant(
        license,
        sibling,
        Operations::None(CreationRights::CLONE),
        Modifiable::No,
    );
    FOREIGN.store(task.get(), Ordering::Release);
    unit::resume(sibling_target).unwrap();
    until(|| PROBED.load(Ordering::Acquire));
    assert_ne!(
        fixture::inspect(UnitQuery::Caller).team,
        fixture::inspect(UnitQuery::Target(target)).team
    );
    unit::resume(target).unwrap();
    until(|| fixture::inspect(UnitQuery::Target(target)).reclaimed);
    assert_eq!(fixture::inspect(UnitQuery::Target(target)).result, Some(42));
    unit::delete(target).unwrap();
    until(|| match mapping.finish() {
        Ok(()) => true,
        Err(error) if error.busy() => false,
        Err(error) => panic!("cross-Team mapping cleanup {error:?}"),
    });
    until(|| match pend::return_(whole) {
        Ok(()) => true,
        Err(error) if error.source.is_busy() => false,
        Err(error) => panic!("Whole cleanup {error:?}"),
    });
    unit::finish(team).unwrap();
    unit::delete(UnitTarget::Team(team)).unwrap();
    pend::return_(source).unwrap();
    cell::delete(slot).unwrap();
    FINISH.store(true, Ordering::Release);
    until(|| fixture::inspect(UnitQuery::Target(sibling_target)).reclaimed);
    assert_eq!(
        fixture::inspect(UnitQuery::Target(sibling_target)).result,
        Some(43)
    );
    unit::delete(sibling_target).unwrap();
    writeln!(
        Console,
        "PASS reply-cross-team local=1 creator=1 sibling=1 sender=1 forged=1 cleanup=1"
    )
    .unwrap();
    writeln!(
        Console,
        "PASS grant-cross-team dormant=1 management-denied=1"
    )
    .unwrap();
}

fn receive(reply: &mut Reply, out: &mut [u8]) -> weave::ReceivedFrame<TaskId> {
    let end = fixture::deadline();
    loop {
        match reply.receive(out) {
            Ok(frame) => return frame,
            Err(ReplyError::Receive(ReceiveError::Pull(error))) if error.source.is_busy() => (),
            Err(ReplyError::Receive(ReceiveError::Open(error))) if error.source.is_busy() => (),
            Err(ReplyError::Receive(ReceiveError::Shut(error))) if error.source.is_busy() => (),
            Err(ReplyError::Receive(ReceiveError::Return(error))) if error.source.is_busy() => (),
            Err(error) => panic!("cross-Team receive {error:?}"),
        }
        assert!(room::clock() < end, "cross-Team reply timed out");
        room::yield_();
    }
}
fn finish(reply: &mut Reply) {
    until(|| match reply.finish() {
        Ok(()) => true,
        Err(ReplyError::Return(error)) if error.source.is_busy() => false,
        Err(ReplyError::Receive(ReceiveError::Return(error))) if error.source.is_busy() => false,
        Err(error) => panic!("cross-Team reply cleanup {error:?}"),
    });
}
extern "C" fn worker() -> ! {
    let target = TaskId::new(FOREIGN.load(Ordering::Acquire));
    assert_ne!(target.get(), 0);
    let mut service = Reply::create(fixture::license(), 64).unwrap();
    // The receiver is still Dormant. Resource delegation grants no execution control.
    service.grant(target).unwrap();
    assert!(
        matches!(unit::resume(UnitTarget::Task(target)), Err(error) if error.source == UnitFail::Denied)
    );
    PROBED.store(true, Ordering::Release);
    let mut bytes = [0; 8];
    let frame = receive(&mut service, &mut bytes);
    assert_eq!(frame.len, 8);
    assert_eq!(frame.metadata, target);
    let token = PendId::from_bytes(&bytes).unwrap();
    let creator = fixture::inspect(UnitQuery::Caller).target;
    let UnitTarget::Task(creator) = creator else {
        unreachable!()
    };
    assert!(Sender::reply(token, creator).is_err());
    let mut sender = Sender::reply(token, frame.metadata).unwrap();
    sender.push(&42u64.to_le_bytes()).unwrap();
    until(|| FINISH.load(Ordering::Acquire));
    finish(&mut service);
    fixture::exit(43)
}
extern "C" fn foreign(arguments: usize, count: usize) -> ! {
    assert_eq!(count, 2);
    // SAFETY: Unit.Create copied these two words onto this task's own stack.
    let args = unsafe { core::slice::from_raw_parts(arguments as *const usize, count) };
    let license = fixture::license();
    let mut parent = Reply::create(license, 64).unwrap();
    parent.grant(TaskId::new(args[0])).unwrap();
    finish(&mut parent);
    let sibling = TaskId::new(args[1]);
    let mut reply = Reply::create(license, 64).unwrap();
    let route = reply.grant(sibling).unwrap();
    let initial = fixture::owned_reply(sibling);
    Sender::new(initial).push(&route.to_bytes()).unwrap();
    let mut bytes = [0; 8];
    let frame = receive(&mut reply, &mut bytes);
    assert_eq!(frame.metadata, sibling);
    assert_eq!(frame.len, 8);
    assert_eq!(u64::from_le_bytes(bytes), 42);
    finish(&mut reply);
    fixture::exit(42)
}
