# Instance handoff v1

This is the bounded caller-owned subset of System #13 for the Hub handoff in
Vanllia #39. It adds `shuyra.system.control.instance.v1` (envelope domain 10)
and `shuyra.system.operator.unit.v1` operations. The enclosing SYS2 version
remains 4; existing domain/tag meanings are unchanged. Select API, Client and
Server from the same full System commit. Kernel, Twine, Weave and pure Unit
pins remain those in the root README and Cargo locks.

| Surface | Caller and authority | Commit and result | Completion / cleanup |
| --- | --- | --- | --- |
| `instance::Create` | Real declared Unit with a delivered `system/control:instance` Clone grant; matching construction policy, caller ownership and subject policy | Reserves an operation record before construction; returns a receipt with a System InstanceId | Completed means dormant native Task plus identity, namespace and declared Supply installation prepared; it does not mean Running or Ready |
| `Start` | Actual instance owner, authenticated by ingress | Records intent, then the shared Instance owner resumes execution | Completed means native resume committed; `Inspect.ready` separately reports readiness |
| `Stop` | Actual instance owner | Requests irreversible closing through the same Instance/Gate | Completed means native stop admitted (or already retired); mappings, grants and backing may still be retained |
| `Retire` | Actual instance owner | Requests closing and retains the operation through cleanup | Completed requires Task reclamation, installation/grant/lease retirement, mapping release, Team deletion and Operator UnitRoot retirement |
| `Inspect` | Actual instance owner | Current serialized Instance observation, including optional Root | Task/Team/Entry coordinates are observations, never management authority |
| `InspectOperation` | Original actual caller, on the original System service | Historical receipt for that caller's nonzero OperationId | Never dispatches the original effect again |
| `Cancel` | Original actual caller of a known operation | Pending work requests instance closing; completed receipts are unchanged | Pending cancellation becomes Cancelled only after native retirement. Unknown is not an unseen-ID cancellation tombstone |
| `Publish` | Owner of the new instance and the stable name; name must be an instance namespace key declared for the target | Revision CAS against the Operator registry; only Active roots can be associated | Replacement changes discovery only; it does not transform, transfer or revoke old consumer Pends |
| Operator `OwnRoot` | Actual querying Task with its independently delivered, live Grant | One cursor page of registry metadata, complete Grant constraints and the caller's recipient-local entrance | Team membership and knowledge of an InstanceId/Mark do not mint a grant |
| Operator `ResolveRoot` / `Relative` / `Find` | Delivered discovery endpoint, permit judgment and native source admission | Resolves a stable association, then derives a constrained actual Pend | Subsequent Consumer-to-Driver RPC goes directly to the Driver |
| Operator root register/advance/associate | Private root entrance, authenticated System controller | Operator owns registry identity, phase and revisions; ordinary Pane/Tile nodes remain unchanged | Operator proves its Pane/grant settlement; System attests execution settlement only after its native Job finishes |
| Supply `Acquire/Inspect/Release/Cancel` | Provider-defined admission against an actual owned source and real caller/target | Existing v4 contract and retained native lease owner | A successful Return proves withdrawal; a device description, numeric target or discovery name grants no rights |

The historical named Control domain 1 and Loader construction domain 4 remain
for existing staged composition. Their Task selectors and `Build/Claim` receipts
are not this contract. New Hub code must use `system_client::instance::Client`,
not the historical Loader client. Pure `loader::Input`/argument values are reused
without borrowing Loader authorization. This increment does not freeze or accept
all historical public SDK surfaces.

## Identity and replay

`InstanceId { system, sequence }` contains the actual System Task and its shared
nonzero, non-reusing Instance sequence. Static and dynamic attempts use the same
Instance storage and native lifecycle owner. A new attempt receives a new
sequence; a replacement System has a different issuer. IDs are scoped to one
Kernel boot. Unknown issuers and old instances cannot select a newer attempt.

Catalog images borrow the immutable boot archive for its actual lifetime;
caller-supplied images retain independent owned snapshots. Both paths use the
same ELF validation and native Job. Construction advances at most eight native
steps or a 250 ms quantum per supervisor round, checking the clock after each
step. A step may itself await its bounded native result. This amortizes ingress
polling without using elapsed time or Busy as proof of completion.

Operation IDs are scoped to the actual System service and native caller. Create,
Start, Stop, Retire and Publish share one journal. The exact original intent
returns its original receipt without repeating effects; a changed intent returns
Conflict. A terminal failure is retained too. There are 256 instance slots and
256 receipt slots for this System lifetime; exhaustion returns Full and never
evicts pending work or reuses an accepted ID. These are explicit bounds, not a
claim of indefinitely renewable capacity.

Keep the submitted Client `Pending` or its transferred `Cleanup` until reply-route
cleanup succeeds. Losing an application reply does not cancel admitted work.
Query its original operation on the original peer. Unknown means no acceptance
record was observed; it does not prove that an already submitted frame cannot
arrive later. A new operation ID is appropriate for a new intent, including
Retire after a completed Create. Transport timeouts do not undo native commits.

## Root ownership and publication

A publishing instance has one ordinary UnitRoot Pane under the structural
Operator root. Declared instance namespace keys/paths lie below that Pane.
Shared placement remains composition policy. A Unit without instance namespaces
needs no UnitRoot. Instance placement no longer installs the old global
`key/iN/path` discovery shape; consumers use OwnRoot or ResolveRoot instead.

System checks the real Task's native creator and Team before registration.
Operator lacks administrative inspection rights over those foreign Teams and
does not infer proof from a serialized ID: it accepts the registration only on
the authenticated System root entrance. Ordinary scoped entrances cannot
register or advance a root. Each actual Task still needs an explicit Grant;
this version installs grants for the initial Task only.

Start OwnRoot with cursor zero and continue with the returned cursor until
None. Cursors are opaque, ordered and never reused; each page checks the live
holder independently. A page is an observation, not a multi-page authority
snapshot. At most one Grant is encoded per response; the discovery response
budget is 16 KiB. This avoids making every discovery call reserve a worst-case
32-Grant response in its native reply owner.

The registry advances Preparing → Active → Closing → Retiring → Retired;
Preparing can close directly on failure. Active follows real readiness.
Transitions use revision checks and retain their original replay receipts.
Closing unbinds associations to this root, preserving the name's owner and a
revision tombstone. It rejects fresh publication, delegation and consumer Read,
including Read through a remembered Entry. Existing consumer Pends keep their
native lifetime until an actual source/branch withdrawal or provider exit.

Namespace installation retains every partial Pane, publication, lease and
authority entrance. Operator retains references to its Authority grant records,
not a duplicate grant state machine or the Kernel's full Pend inventory.
Retired requires those grant records settled and the root Pane removed, plus
System's authenticated execution-completion commit after native Job cleanup.
Busy retains the corresponding owner. Positive native exit evidence, including
a retained terminal observation, establishes owner death; an unavailable query
does not. Owner death requests closing even before the target Task exists.
System's shutdown predicate
includes every dynamic instance, including unclaimed partial attempts.

For recovery, create/start the replacement, observe readiness and Publish with
the current association revision. A stale CAS fails. An exact repeated Publish
returns its recorded receipt even if subsequent discovery has changed; query
ResolveRoot for current state. Replacing a live association leaves old consumer
Pends attached to the old provider until that provider is explicitly retired.

## Resource handoff

System supplies its genuinely owned catalog resources, including the restricted
Instance endpoint. A provider such as Hub supplies its own actual native source
to the Driver selected from the authenticated Instance observation. The native
lease owner checks caller, target, maximum rights and lineage. System does not
proxy resources it does not own or infer hardware authority from IDs.

The executable fixture in `tests/guest/handoff` runs ordinary controller,
observer and Driver Teams against the production Server. It exercises narrow
System endpoint delivery, Hub-like native source delivery, Driver publication,
direct RPC, live root replacement, revocation and retirement. It uses CPU Signs
and Clone queues; it supplies no PCI/BAR/IRQ/External-device proof.

## Remaining acceptance

S1/S2 remain open until their complete checklists and downstream signing in
System #13 / Vanllia #39 pass. This v1 subset does not implement selected-owner
transfer, extra-Task root-grant admission, lifecycle Watch, live System takeover,
or a complete queue/recipient-budget and source-failure campaign. The anonymous
image path retains native source validation but this new handoff fixture covers
declared Unit inputs. It is a no_std native ELF fixture, not a Toolchain std Unit.
Hub must first select this exact provider graph and verify its real consumer;
provider-first merge/pin precedes changing Hub or Vanllia production pins.
