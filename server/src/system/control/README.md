# Control ownership

`Control` is the serial coordinator for program selection, Instance progress,
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
