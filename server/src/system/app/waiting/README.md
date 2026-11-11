# Supervisor waiting

`Waiting` owns a Kernel Sign and its installed subscription set. It replaces the
old Resource Pile and Tole interface. Identity uses the same implementation for
its native multi-endpoint loop.

`apply` receives Clone read/write endpoints and explicit generation-qualified
interests. Readiness uses Pull for reads and Push for writes. Duplicate interests
are merged. Conflicting generations for the same source are rejected before any
Kernel changes. Storage is reserved before modifying subscriptions.

Successful detach removes its local record. Failed detach retains it. Successful
attach records the subscription; a later failure preserves all earlier changes.
A subsequent apply reconciles that actual partial state. This fixes the old
behavior that ignored detach errors and forgot the corresponding route.

`await_` preserves Kernel event and failure metadata and accepts absolute Wait
values. Readiness does not mean a pending operation completed. `finish` prevents
further use before returning the Sign. Failed return retains ownership and all
records for retry. Drop is best effort.

The real one-/four-Hart fixture checks deduplication, source replacement, a later
attach failure after earlier progress, successful subsequent reconciliation,
conflicting generation rejection before effects, idempotent finish and caller
resource baseline. The full production Identity loop also runs through it.

The old supervisor interest producers and startup installer still use their
retired ABI types and constructor shape. They remain explicit compile failures
on the draft migration branch until their own Kernel migration; no conversion
shim is added to reinterpret those old handles as current authority.
