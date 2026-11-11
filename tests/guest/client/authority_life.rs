//! State publication follows real native attach, Accord and Return receipts.
use crate::{
    fixture::{self, Console, until},
    operator_authority::{Authority, CleanupStep, GrantRef, GrantSubmission, RootGrant},
    waiting::Waiting,
};
use core::fmt::Write as _;
use kernel_api::model::{
    clone_view::CloneLimits,
    identity::{Mark, PendId, TaskId},
    modifiable::Modifiable,
    pend::{CloneRights, Operations},
};
use kernel_client::call::{clone_view, pend};
use system_client::api::{
    self,
    operator::{Entry, EntryTarget, GrantId, GrantSpec, Ops, Scope},
    value::Optional,
};
use twine::view::clone::Sender;

pub fn run(license: PendId, root: TaskId, holder: TaskId) {
    let baseline = fixture::active();
    let create = || {
        clone_view::create(
            license,
            Mark::NONE,
            CloneLimits {
                max_len: 1024,
                max_messages: 1,
                max_bytes: 1024,
            },
        )
        .unwrap()
    };
    let ingress = create();
    let entrance = create();
    let mut waiting = Waiting::new(license).unwrap();
    let mut authority = Authority::new(root.into(), root.into());
    authority
        .bootstrap_root(RootGrant {
            system: root.into(),
            operator: root.into(),
            entrance: ingress,
        })
        .unwrap();
    let id = GrantId::new(1).unwrap();
    let reference = GrantRef {
        caller: root.into(),
        via: ingress,
        id,
    };
    let (_, key) = authority
        .grant(
            GrantSubmission {
                caller: root.into(),
                via: ingress,
                spec: GrantSpec {
                    id,
                    parent: Optional(None),
                    holder: holder.into(),
                    anchor: Entry {
                        operator: root.into(),
                        target: EntryTarget::Node(api::EntryId::new(0)),
                    },
                    scope: Scope::Subtree,
                    ops: Ops::BIND,
                    permit: Optional(None),
                    source: Optional(None),
                    source_mark: Optional(None),
                },
            },
            &api::Path::new("service").unwrap(),
        )
        .unwrap();
    authority.set_entrance(reference, entrance).unwrap();
    waiting.apply(&[entrance], &[], &[]).unwrap();
    authority.attached(key).unwrap();
    let mut remote = None;
    until(|| match fixture_grant(entrance, holder) {
        Ok(grant) => {
            remote = Some(grant);
            true
        }
        Err(error) if error.source.is_busy() => false,
        Err(error) => panic!("authority Accord: {error:?}"),
    });
    authority
        .delivered(reference, remote.unwrap().into())
        .unwrap();
    let receipt = authority.status(reference).unwrap();
    assert_eq!(receipt.entrance.0, Some(remote.unwrap().into()));
    assert_eq!(receipt.state.0, api::operator::GrantState::Granted);
    assert!(authority.live(key));
    assert!(authority.grant_at(entrance, holder.into()).is_some());
    authority.revoke(reference).unwrap();
    assert_eq!(authority.status(reference).unwrap().entrance.0, None);
    assert!(authority.grant_at(entrance, holder.into()).is_none());
    for item in authority.revoke_plan(reference).unwrap() {
        waiting.apply(&[], &[], &[]).unwrap();
        authority
            .cleanup_step(item.key, CleanupStep::Detach)
            .unwrap();
        assert!(authority.closed(item.reference).is_err());
        until(|| match pend::return_(item.entrance.unwrap()) {
            Ok(()) => true,
            Err(error) if error.source.is_busy() => false,
            Err(error) => panic!("authority Return: {error:?}"),
        });
        authority
            .cleanup_step(item.key, CleanupStep::Return)
            .unwrap();
        authority.closed(item.reference).unwrap();
    }
    assert!(!authority.live(key));
    assert!(Sender::new(entrance).push(b"closed").is_err());
    waiting.finish().unwrap();
    pend::return_(ingress).unwrap();
    assert_eq!(fixture::active(), baseline);
    writeln!(
        Console,
        "PASS authority-life attach=1 grant=1 revoke=1 return=1 cleanup=1"
    )
    .unwrap();
}

fn fixture_grant(
    source: PendId,
    target: TaskId,
) -> Result<PendId, erra::Error<kernel_api::abi::call::pend::PendFail>> {
    use kernel_api::model::pend::{AccordRequest, AccordTarget, Distributable, Limits, PendRange};
    let request = AccordRequest {
        src: source,
        target: AccordTarget::Task(target),
        mark: Mark::NONE,
        limits: Limits {
            operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
            modifiable: Modifiable::Yes,
            distributable: Distributable::No,
            range: PendRange::Inherit,
        },
    };
    pend::accord(fixture::addr(&request.to_bytes()))
}
