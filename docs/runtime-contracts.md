# Runtime contracts

System reads the VNI1 bundle, validates VNU1 records and image space, enforces
required inclusions, and derives startup order from selected declarations.
Requested templates remain available without becoming initial instances. The
host passes no serialized Deployment.

Runtime declarations share owned arrays through Arc and borrow strings only
from genuine immutable initrd storage. `server/Cargo.toml` declares generated
runtime resources; these are not files required inside initrd. Undeclared
missing file resources must still fail packaging.

Construction publishes binding only after the job and native task/team IDs
exist. Readiness uses the actual readiness result. Exit frames reuse native
Join receipts, preserve cause/reason and publish once. Anonymous image jobs use
`image` plus native task/team IDs as their observation identity.

A dead execution domain may still have a live owner's retained team receipt.
Keep installation metadata until the receipt becomes Consumed/Reported after
Oust; only then publish Retired and remove the job. Losing a controller row does
not establish retirement. Delivery failures propagate; publication state
advances only after delivery succeeds. Native exit and retirement are distinct
observations, and host verdicts require both.

Early owner exit is a known incomplete path: pruning its controller row can
prevent publication of Retired. This fails closed. Do not reinterpret an absent
receipt or a diagnostic string as successful resource reclamation.
