# Native application composition

`run` interprets the immutable schema-2 catalog and retains every native owner
until its explicit completion boundary. It uses the existing production heap,
Operator/Identity bootstrap, Loader and Supply backend. There is no ECS or old
Env/Wire/Mold adapter in the production graph.

- `services` owns bootstrap publications and uses one submitted Pending followed
  by explicit reply cleanup. An indeterminate effect terminates the Team while
  owners remain present; it never invents a successful rollback.
- `endpoints` owns authenticated ingress for existing v4 Control, construction,
  export and Supply roles. Queue access is narrowed to Push/Peek. Caller Task
  metadata comes from the native transport, independently of body IDs.
- `namespace` owns each instance's grants, publications, delegation roots and
  containers, including partial installation. Pending cleanup retains its
  mutation ID. Operator remains the mutation/authorization authority.
- `control::Instance` retains the actual Loader Job, installation and retirement
  gate. Stop is separate from Task reclaimed, Team Delete and source release.
  Named program intent refers to an instance; it does not replace that owner.

The catalog is the trusted admission policy. Internal sources are registered as
`system/control` (State/Mint/Embark/Debark/Ruin/Observe/Recover and `create`),
`system/identity` (declared non-installer identity roles and `export`), `system`
(`supply`) and `kernel/boot` (`dtb`, `initrd`, `memory`, genuine `region-<base>`
ledger grants). Source names/marks locate already-owned roots and never create
hardware authority. Maximum limits constrain both leases and publications.
Old fixed PCI/BAR/IRQ/IOMMU source conventions and device cleanup hooks are not
implemented by this composition. Their replacement belongs to Hub.

Construction policy is matched against the authenticated live caller's actual
Team and declaration, not the selected owner's identity. Image import checks
an actual caller-lent Share and copies volatile bytes into a private validated
ELF snapshot. Build retries use `(caller, CreateId)` plus full request equality;
a conflicting intent is rejected. Receipts are bounded (256 attempts), IDs do
not wrap/reuse, and exhaustion fails explicitly. Requester and owner receive
separate local Resolve coordinates. Explicit owner Claim retains the attempt;
abandoned unclaimed construction expires after five seconds or requester exit.
These v4 routes are implementation candidates, not a frozen S2 contract.

Normal startup respects selected dependencies. Failure closes dynamic attempts
before canonical providers; declared consumers retain providers until retirement
settles. Unsettled safety gates fail closed. Cleanup time bounds cause a Team
exit with its owners retained, not early source retirement.

The complete binary builds in both profiles and release application scenarios
run under the qualified one-Hart QEMU platform. The unoptimized application's
Operator fault remains unresolved (see the [application scenario](../../../../tests/guest/system/README.md)).
Public operation/instance identity and lifetime semantics, returned UnitRoot,
Create cancellation/replay pressure, selected-owner death, retirement-provider
failure, supervision recovery, and std User integration require further focused
validation before S1/S2/S3 can be accepted. The old daemon/session/application
policy hooks and test callback injections are not compatibility layers here.

## Retired path mapping

| Former production path | Current enforcing owner |
| --- | --- |
| ECS app/scene/install/execute and launch forwarding | `app::run`, native `Services`, authenticated `Endpoints` |
| control/unit state and lifecycle fragments | Program intent referring to the retained `control::Instance` |
| old Control construction endpoint / Loader service | Control admission and request receipts; native Loader Job owns effects |
| assembly static/work/supply/hooks | Instance Installation and provider retirement Gate |
| old app namespace install/retire fragments | Namespace Installation with retained grants/publications/roots |
| identity namespace/export adapters | Identity service plus declared Control export admission |
| fixed hardware supplies and source wrappers | Native Registry/Source; hardware policy remains with Hub |
| server test-support and old Loader ECS harness | `tests/historical/`; active native tests stay under `tests/` |

This mapping does not imply full behavioral parity. The old private callback
harness, hardware hooks and SDK clients remain historical. The remaining public
semantic and runtime gaps listed above keep the PR Draft.
