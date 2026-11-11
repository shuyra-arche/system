# Native lease owner

`native` is the Kernel9 resource-lease backend for the ongoing M4 migration.
The old Supply service and application integration in this directory still need
porting; this backend is not an old-Pie forwarding layer or a complete service.

Registration borrows a source Pend and owns a narrowed local registration root.
Explicit caller, target, maximum rights and description policy precede effects.
Each accepted lease then owns a distinct private root and recipient-local delivery.
A page window is resolved on the private root; delivery inherits that resolved
window rather than applying its relative offset twice. Private roots are
internally distributable, while recipient distribution stays within the public
configured maximum. Kernel revalidates actual ancestry and range at Accord.

Native ingress must supply the actual caller. A lease key combines that caller
and its ID. Identical retries return the retained delivery, changed retries
conflict, cancellation before acquisition remains closed, and delayed IDs are
accepted while their history is retained. At capacity, completed history may be
compacted behind per-caller closed high-water marks; stale IDs cannot recreate
old grants. Bounds are 64 definitions/callers and 256 lease/cancellation records.

`advance` makes one fair lease transition. Failure/cancellation retains partial
roots until withdrawal finishes. `finish` first retires all lease descendants,
then registration roots, without returning the provider's original borrowed
source. Successful native Return proves withdrawal. An ancestor may have
completed removal first: a missing private local record is accepted only after
native inspection, never merely because a Return call failed or a Task stopped.

The runtime guest verifies real derivation, exact rights, requester/recipient
scope, replay/conflict, out-of-order IDs, partial cancellation, withdrawal and
provider-source retention. It separately maps an eight-page source, narrows the
registration to pages 1..4, requests page 1 of that window, and verifies the
recipient reads original page 2 with one-page span. Multi-root collections,
provider callbacks, the Supply RPC loop and Control retirement gates remain to
be integrated; CPU/QEMU evidence does not establish device quiescence.
