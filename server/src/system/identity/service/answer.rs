//! Endpoint admission precedes every state read or write.
use crate::system::identity::book::{
    Anchor, Association, BindingRequest, IdentityBook, InstallAction, InstallationRequest,
    Membership, ReferenceQuery, Selection,
};
use system_protocol::{
    PendId, TaskId,
    identity::{
        self, CoalitionId, Fail, Grant, MatchReply, PageTarget, PrincipalId,
        operations::{DecodedRequest, Response},
    },
    value::{Optional, Outcome},
};
use weave::Coded;

/// From the serving endpoint and authenticated Clone metadata, never the body.
pub struct Context {
    pub from: TaskId,
    pub endpoint: PendId,
    pub grant: Grant,
}

pub fn answer(book: &mut IdentityBook, context: Context, request: DecodedRequest) -> Response {
    let Context {
        from,
        endpoint,
        grant,
    } = context;
    macro_rules! route {
        ($variant:ident, $grant:ident, $result:expr) => {
            Response::$variant(
                if grant != Grant::$grant || from == TaskId::NONE || endpoint == PendId::NONE {
                    Outcome::Failure(Fail::Denied)
                } else {
                    match $result {
                        Ok(value) => Outcome::Success(value),
                        Err(fail) => Outcome::Failure(fail),
                    }
                },
            )
        };
    }
    match request {
        DecodedRequest::Resolve(request) => {
            route!(Resolve, Resolve, Ok(Optional(book.resolve(request.task))))
        }
        DecodedRequest::Matches(request) => route!(
            Matches,
            Matches,
            book.matches(request.task, request.selector)
                .map(|value| MatchReply {
                    value: Coded(value)
                })
        ),
        DecodedRequest::Same(request) => route!(
            Same,
            Same,
            Ok(MatchReply {
                value: Coded(book.same(request.left, request.right))
            })
        ),
        DecodedRequest::Sire(principal) => route!(Sire, Sire, book.sire(principal).map(Optional)),
        DecodedRequest::Heir(request) => {
            route!(Heir, Heir, book.heir(request.parent, request.child))
        }
        DecodedRequest::Amid(request) => {
            route!(Amid, Amid, book.amid(request.principal, request.coalition))
        }
        DecodedRequest::Members(request) => route!(
            Members,
            Members,
            book.page::<PrincipalId>(PageTarget::Members(request.coalition), request.cursor.0)
        ),
        DecodedRequest::Memberships(request) => route!(
            Memberships,
            Memberships,
            book.page::<CoalitionId>(PageTarget::Memberships(request.principal), request.cursor.0)
        ),
        DecodedRequest::Adopt(subject) => route!(
            Adopt,
            Adopt,
            book.narrow_own(
                from,
                Selection {
                    subject,
                    anchor: Anchor::Keep
                }
            )
            .and_then(|()| book.resolve(from).ok_or(Fail::Denied))
        ),
        DecodedRequest::Waive(()) => route!(Waive, Waive, book.waive(from)),
        DecodedRequest::Restrict(subject) => route!(
            Restrict,
            Restrict,
            book.narrow_own(
                from,
                Selection {
                    subject,
                    anchor: Anchor::Move
                }
            )
            .and_then(|()| book.resolve(from).ok_or(Fail::Denied))
        ),
        DecodedRequest::Derive(parent) => route!(Derive, Derive, book.derive(from, parent)),
        DecodedRequest::Found(id) => route!(Found, Found, book.found(from, id)),
        DecodedRequest::Admit(request) => route!(
            Admit,
            Admit,
            book.admit(
                from,
                Membership {
                    principal: request.principal,
                    coalition: request.coalition
                }
            )
        ),
        DecodedRequest::Expel(request) => route!(
            Expel,
            Expel,
            book.expel(
                from,
                Membership {
                    principal: request.principal,
                    coalition: request.coalition
                }
            )
        ),
        DecodedRequest::Bind(request) => route!(
            Bind,
            Bind,
            book.bind(
                from,
                BindingRequest {
                    task: request.task,
                    install: request.install
                }
            )
            .and_then(|()| book.resolve(request.task).ok_or(Fail::Denied))
        ),
        DecodedRequest::Unbind(request) => route!(Unbind, Unbind, book.unbind(from, request.task)),
        DecodedRequest::Activate(request) => route!(
            Activate,
            Activate,
            book.activate(from, (request.task, request.coalitions))
                .and_then(|()| book.resolve(request.task).ok_or(Fail::Denied))
        ),
        DecodedRequest::Install(request) => route!(
            Install,
            Install,
            book.installation(InstallationRequest {
                endpoint,
                caller: from,
                id: request.id,
                action: InstallAction::Apply(BindingRequest {
                    task: request.task,
                    install: request.install
                }),
            })
            .map(status)
        ),
        DecodedRequest::Inspect(id) => route!(
            Inspect,
            Install,
            book.installation(InstallationRequest {
                endpoint,
                caller: from,
                id,
                action: InstallAction::Inspect
            })
            .map(status)
        ),
        DecodedRequest::Cancel(id) => route!(
            Cancel,
            Install,
            book.installation(InstallationRequest {
                endpoint,
                caller: from,
                id,
                action: InstallAction::Cancel
            })
            .map(status)
        ),
        DecodedRequest::Retire(id) => route!(
            Retire,
            Install,
            book.installation(InstallationRequest {
                endpoint,
                caller: from,
                id,
                action: InstallAction::Retire
            })
            .map(status)
        ),
        DecodedRequest::Reference(request) => route!(
            Reference,
            Reference,
            book.reference(ReferenceQuery {
                object: request.object,
                name: request.name
            })
            .map(Optional)
        ),
        DecodedRequest::Associate(request) => route!(
            Associate,
            Install,
            book.associate(
                from,
                Association {
                    object: request.object,
                    name: request.name,
                    entry: request.entry
                }
            )
        ),
    }
}

fn status(value: crate::system::identity::book::InstallStatus) -> identity::InstallStatus {
    identity::InstallStatus {
        state: Coded(value.state),
        binding: Optional(value.binding),
        failure: Optional(value.failure.map(Coded)),
    }
}
