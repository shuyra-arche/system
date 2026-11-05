# Requested Unit lifecycle receipts

The product's public Login fixture creates Shell after authentication. The
post-merge clean build entered that Shell, but System originally emitted typed
lifecycle frames only for startup instances. The host could not verify Shell's
native exit and reclamation through the existing receipt protocol.

Construction jobs now keep explicit publication state. Binding is emitted after
the constructed instance has a tracked job and native task/team IDs. Readiness
uses the existing readiness result. Native exit frames reuse the actual Join
receipts already read by the lifecycle controller; repeated peeks emit one exit.
Anonymous image jobs use the observation label `image`, distinguished by native
task/team IDs. No static Unit registry or kernel event logger is added.

A dead execution domain can still have a live owner's retained team receipt.
Keep its installation metadata until Receipt is Consumed/Reported after Oust;
only then publish Retired and remove the job. Losing a controller row does not
invent a Retired frame. Frame delivery errors propagate, and publication state
advances only after successful delivery.

The smoke covers the normal live-owner sequence: Shell exits and is released
before Login exits. Early owner exit is not covered. The current controller may
prune its dead row before the job publishes a consumed receipt; that path can
lose a Retired frame and fails closed. This patch does not claim owner-failure
or old recovery-scenario acceptance.

The isolated `tests/instance-events` host workspace compiles the actual
publication-state module and covers repeated native observations, cause/reason
preservation, invalid transition rejection and failed delivery retries. The
existing two dynamic-Unit reader tests also pass. Validation uses installed
nightly-2026-10-09; historical toolchain pins and old scenarios are unchanged.

The paired Vanllia Draft adds one ignored single-core Rust host smoke test. It
uses the exact product artifact without adding a test Unit, authenticates using
the existing public fixture, checks requested Shell creation, evaluates
`(+ 314000 159)` as `314159`, then requires native Shell/Login zero exits, all
retirements, successful System completion and natural VM exit zero. Exact input
hashes and results are kept with that intake's validation record.
