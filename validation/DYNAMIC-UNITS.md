# Dynamic runtime selection

System's package metadata declares the runtime resources it generates from boot
facts. The host does not require machine inventory or platform capabilities to be
files in initrd, and undeclared missing file resources still fail packaging.

The host passes no Deployment file. System reads the VNI1 bundle, validates each
VNU1 record and image space, enforces the required inclusion set, and derives
startup order from selected declarations. Requested templates remain available
for construction without appearing among initial instances.

Ownership uses Arc-backed native declarations. Records borrow immutable initrd
strings; arrays are owned and shared. Retirements emit a native TaskExit before
Oust and a retirement frame after cleanup completes. Host verdicts require both.

Validation on nightly-2026-10-09, without local Cargo patches:

- Actual RV64 `system-server` / `system` executable built using public Unit
  ad50b803c3bd5cd2ef90010d80badf23551e88d5 and bundle reader
  ee9357070996e2e559095904ff137a289a4ca156, preserving Env's Kernel ABI facade
  and using the canonical capsule reader. Formal root Cargo.lock is tracked.
- Two new host tests compile the actual catalog, decoder and startup selection
  against generated binary bundle fixtures: requested-instance exclusion,
  missing dependencies, boot image mismatch, inactive readiness and selected cycles.
- Existing server warnings remain. Old test-support and historical scene tests
  are not migrated and no full old-suite green claim is made.

No QEMU, guest runtime, SMP or performance acceptance was run. Native event
delivery and runtime resource reclamation still require authorized guest testing.
