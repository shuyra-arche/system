# Native Operator lifecycle

The serving Task owns namespace state, scoped grant receipts, request ingress
and subscriptions. Config borrows a current Kernel creation license and a
controller-owned shutdown Sign with Listen/Consume/Route rights. Construction
returns explicit controller-local discovery, root authority and setup grants.
It does not scan Marks or establish legacy persistent request/reply sessions.
The retired draft setup Guest operation (tag 2) is no longer accepted.

Only the configured controller can enable root authority or replace the trusted
Identity query bundle. Root admission is structural and has no wire GrantId.
Wired query grants are borrowed from the controller; the controller retains
those loan obligations. A successful replacement receipt follows completion of
the old TaskQuery's per-call cleanup. These grants must remain valid until a
replacement is accepted or the service stops. Unknown identity facts deny a
resource request with Unjudged; Public needs no identity query.

Every namespace mutation checks actual ingress, sender, live grant ancestry,
operation rights, remaining scope, binding revision and publication restrictions.
Publication imports additionally inspect native lender, owner, Mark and permitted
Kernel child limits. Caller-declared resource identity is a retry correlation,
not source authority. Foreign or protected coordinates cannot make the service
Return another caller's live publication. Candidate inspection and failed native
returns retain their owning records for subsequent cleanup.

Mutation history is bounded and keyed by native ingress, authenticated caller
and nonzero operation ID. Fingerprints include revision, source, policy and
rights. A committed or failed request cannot execute again. Fresh resource grants
submitted with a replay are settled separately. Cancellation of unseen work
installs a replay fence; cancellation never undoes a committed mutation.

Grant creation retains Pending until a native ingress is bound, subscribed and
actually granted to the declared holder. GrantStatus carries that holder-local
coordinate; the issuer cannot use it as a local Pend. Revocation immediately
disables logical descendants. Retirement processes descendants before parents,
detaches ingress, settles publication roots, completes native Pend Return, and
only then closes each grant receipt. Native Busy retains the same responsibility
and causes another bounded progress pass rather than waiting for unrelated IPC.
Retirement jobs rotate as well: an independently issued grant can own a child
below another grant's namespace anchor. That nonempty anchor must stay retained
without preventing the child grant's own retirement from making progress.

Unbind and Replace remove a namespace binding while retaining previously granted
consumer access. Their old publication roots remain explicitly retained. Revoke
withdraws those roots and their native descendants, including copies delivered
before an Unbind. At most 256 active/retained import roots are admitted, plus one
cleanup overflow slot. ABI 9 supplies no child-quiescence observation, so a retained
root stays charged until upstream withdrawal is observed or the service stops.
The implementation does not invent a safe early-reclamation proof. A resource
withdrawn upstream is pruned when discovery observes its death.

Watch admission requires a bound actual caller and a Clone endpoint owned and
lent by that caller. Accepted subscriptions continue until their endpoint is
withdrawn, preserving the original durable admission behavior. Events are best
effort: a full subscriber queue can leave sequence gaps and never blocks a
namespace mutation. Consumers check the native event sender and reconcile by
lookup/list after gaps. Each service round handles at most one frame per ingress
and rotates its starting position. Reply delivery failure is reported after the
mutation and never redispatches it or terminates the service.

Finish stops admission, closes wait groups and retained ingress deliveries,
settles subscriptions/publications and returns every owned ingress root. Partial
construction, progress failure and teardown failure retain a Service owner.
Destruction is best effort and is not a retirement receipt.

The actual production service is compiled by the host fixture in
`tests/operator-policy` and exercised in a real ABI 9 guest by
`tests/guest/client/run.py` on one and four Harts. That scenario uses distinct
controller, Operator and publishing Tasks and verifies scoped/forged-source
rejection, namespace association, event sources, retained consumer access after
Unbind, revocation, upstream withdrawal, replay after reply failure and removal,
independent-anchor retirement without head-of-line blocking, Task exit results
and resource baselines. The existing independent-Team scenario
continues to reproduce Kernel #140; same-Team service results do not prove
ordinary-Unit cross-Team RPC or an assembled System boot. Supervisor wiring and
other System services are still being ported.
