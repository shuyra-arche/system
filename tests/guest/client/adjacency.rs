//! Exercise actual cross-Team grants in both System worker/Unit directions.
use crate::fixture::{self, Console, addr, until};
use core::{
    fmt::Write as _,
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use kernel_api::{
    abi::call::pend::PendFail,
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{
            Allocation, CellCreate, CellInfo, CellObject, CellPosition, CellSource, CreatedCell,
            MemoDestination, Placement,
        },
        identity::{PendId, TaskId},
        modifiable::Modifiable,
        pend::{CreationRights, NarrowRequest, Operations, PendInfo},
        unit::{ControlId, ProgramKind, UnitCreate, UnitQuery, UnitTarget},
    },
};
use kernel_client::call::{cell, pend, room, share_view, unit};
use twine::view::clone::{Reply, ReplyError};

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
    let create = CellCreate::Slot(Allocation::new(pages * PAGE_SIZE, PAGE_SIZE).unwrap());
    let CreatedCell::Slot(slot) = cell::create(license, addr(&create.to_bytes())).unwrap() else {
        panic!("Slot expected")
    };
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
        kind: ProgramKind::User,
        guard: ControlId::NONE,
    };
    let UnitTarget::Team(team) = unit::create(addr(&create.to_bytes())).unwrap() else {
        panic!("Team expected")
    };
    let create = CellCreate::Memo {
        source: CellSource::Pend {
            token: source,
            offset: 0,
            count: pages,
        },
        destination: MemoDestination::Internal {
            team,
            placement: Placement::exact(VirtAddr::new(start.addr()), PAGE_SIZE).unwrap(),
            access: 10,
        },
    };
    let CreatedCell::Memo { token: memo, .. } =
        cell::create(PendId::NONE, addr(&create.to_bytes())).unwrap()
    else {
        panic!("Memo expected")
    };
    for page in 0..pages {
        let mut position = CellPosition {
            object: CellObject::Memo(memo),
            page,
            generation: 0,
        };
        let mut bytes = [0; CellInfo::WIDTH];
        cell::inspect(
            addr(&position.to_bytes()),
            VirtAddr::new(bytes.as_mut_ptr().addr()),
        )
        .unwrap();
        position.generation = CellInfo::from_bytes(&bytes).unwrap().generation;
        cell::publish(addr(&position.to_bytes())).unwrap();
    }
    let arguments = [creator.get(), sibling.get()];
    let create = UnitCreate::Task {
        team: Some(team),
        entry: VirtAddr::new(foreign as *const () as usize),
        args: VirtAddr::new(arguments.as_ptr().addr()),
        count: 2,
        stack: VirtAddr::new(0),
    };
    let target = unit::create(addr(&create.to_bytes())).unwrap();
    let UnitTarget::Task(task) = target else {
        unreachable!()
    };
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
    pend::return_(memo).unwrap();
    unit::finish(team).unwrap();
    unit::delete(UnitTarget::Team(team)).unwrap();
    pend::return_(source).unwrap();
    cell::delete(addr(&CellObject::Slot(slot).to_bytes())).unwrap();
    FINISH.store(true, Ordering::Release);
    until(|| fixture::inspect(UnitQuery::Target(sibling_target)).reclaimed);
    assert_eq!(
        fixture::inspect(UnitQuery::Target(sibling_target)).result,
        Some(43)
    );
    unit::delete(sibling_target).unwrap();
    writeln!(
        Console,
        "PASS reply-adjacency local=1 creator=1 sibling-denied=1"
    )
    .unwrap();
    writeln!(Console, "PASS grant-adjacency worker-to-unit-denied=1").unwrap();
}

extern "C" fn worker() -> ! {
    let target = TaskId::new(FOREIGN.load(Ordering::Acquire));
    assert_ne!(target.get(), 0);
    let mut grant = Reply::create(fixture::license(), 64).unwrap();
    assert!(
        matches!(grant.grant(target), Err(ReplyError::Accord(error)) if error.source == PendFail::Denied)
    );
    grant.finish().unwrap();
    PROBED.store(true, Ordering::Release);
    while !FINISH.load(Ordering::Acquire) {
        room::yield_();
    }
    fixture::exit(43)
}

extern "C" fn foreign(arguments: usize, count: usize) -> ! {
    assert_eq!(count, 2);
    // SAFETY: Unit.Create copied these two words onto this task's own stack.
    let args = unsafe { core::slice::from_raw_parts(arguments as *const usize, count) };
    let license = fixture::license();
    let mut parent = Reply::create(license, 64).unwrap();
    parent.grant(TaskId::new(args[0])).unwrap();
    parent.finish().unwrap();
    let mut sibling = Reply::create(license, 64).unwrap();
    assert!(
        matches!(sibling.grant(TaskId::new(args[1])), Err(ReplyError::Accord(error)) if error.source == PendFail::Denied)
    );
    sibling.finish().unwrap();
    fixture::exit(42)
}
