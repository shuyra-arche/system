# Protocol v4 ingress

`Endpoint` borrows a native Clone queue and owns its Sign watch, receive buffers
and retained deliveries. Create the queue in the serving Task, grant clients
PUSH and PEEK with Modifiable rights, then bind the matching service alias.

`serve_once` uses Weave's exact request decoding and route-before-dispatch
boundary. The SYS2 version and service must match. `Sender::reply` validates
the reply grant against the actual Clone sender, including owner, lender,
active status and the exact non-distributable Push authority. The handler gets
the authenticated caller and decoded operation body.

Only receive Busy conditions before dispatch can restart reception. A retained
payload with native mapping Pending/Busy is retried without waiting for another queued frame.
One absolute `Wait` bounds this reception. The handler runs at most once. A
reply encoding or send failure is reported after possible service side effects.
It never causes automatic redispatch or cancellation.

Call `finish` before the queue owner returns its root. Failed cleanup retains
the watch or delivery for retry. Returning the root is a separate action that
withdraws clients' queue grants. Ingress does not infer resource reclamation
from service completion or a successful send.

Multiplexers using Poll must check `needs_retry` after NotReady: a retained
payload with native mapping Pending/Busy or Return Busy needs another receive attempt without a new queue
event. No request handler has run for that pending receive.

Each endpoint receives an explicit Task-local `system_client::Context`, shared
with that Task's clients and memory users. It supplies the actual Internal Nest
and one monotonic native RequestId sequence; a new endpoint must not reset it.

Current ABI 11 Client/Ingress runtime coverage is the same-Team scenario in
`tests/guest/runtime`. Identity/Operator host checks compile their native service
loops. The older comprehensive Client guest still needs its remaining ABI 11
port; its historical success does not prove this revision's cross-Team or full
bootstrap behavior. `system-protocol` names the workspace API v4 while the
unmigrated application still carries an API v1 self Git pin.
