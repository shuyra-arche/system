# Control ownership

`Control` is the serial coordinator for canonical attempt admission, Instance progress,
startup and close ordering, and native effects. `Instance` owns one execution
attempt and its installation and reclamation lifecycle. The operation journal
owns the newer operation receipts; it does not own Instance resources.

`construction::Ledger` owns the older caller-scoped Build receipts. A caller
and Create ID identify one immutable request: replaying the same request returns
its retained result, while reusing the ID for a different request is denied.
The ledger preallocates its bounded capacity. Control serializes admission:
replay checks caller and ID before any construction effects, then insertion
asserts that the ID is still fresh. Records remain retained after claim or
expiry outcomes. Each record keeps its selected Instance, claim state, deadline,
and recipient-local Resolve grants so retries return the same grant. Control
decides when an Instance transition is needed and applies it through the sole
Instance owner; the ledger does not own native Task or Team effects.

`construction::snapshot` validates and privately copies an offered image grant
for Control's construction admission. It has no receipt state and does not grant
authority over the source or change receipt ownership.

`program::Program` owns canonical selection and retained demand as one selection
state: Never, Requested or Stopped. Stopping preserves the selected Instance;
intent never proves startup or retirement. Program owns resident/transient exit
eligibility and declared supervision policy. Control still authenticates the
actual caller and consults physical Instance evidence before admission/recovery.

`instance::Installation` retains identity, namespace grants/publications,
Supply leases and readiness roots through partial setup and acknowledged
cleanup. Its namespace owner is separate from the global Layout/sequence planner.
`instance::retirement::Gate` retains provider requests, reply cleanup and roots
until the actual target's safe-retirement receipt permits Stop. Neither a moved
file nor a stopped Task substitutes for that receipt. Instance's owner is fixed
at construction and exposed only as an observation to Control.

Host owner contracts now live in `server/tests/` with explicit App, Control,
Identity, Loader, Operator, Runtime and Supply targets. The complete baseline test/support migration inventory is in
[`docs/test-inventory.md`](../../../../../docs/test-inventory.md); remaining
Identity/Operator hooks and historical assertion ports are tracked there.

`installs::Journal` correlates caller-scoped InstallIds with immutable requests or
cancellation tombstones. It owns no native root or physical status. Each accepted
post-Prepared delivery belongs to `instance::Installation` alongside declared
Supply leases. That owner publishes partial work before effects, gates actual
Start on delivery/settled cancellation, and completes all private branches before
Instance Retired. Source registration identity, Instance management ownership and
native target checks are serialized by Control at admission; Kernel rechecks
actual authority during effects. See [Supply](../../../../../docs/supply.md).
