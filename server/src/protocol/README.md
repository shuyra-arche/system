# Protocol v2 ingress

`Endpoint` borrows a native Clone queue and owns its Sign watch, receive buffers
and retained deliveries. Create the queue in the serving Task, grant clients
PUSH and PEEK with Modifiable rights, then bind the matching service alias.

`serve_once` uses Weave's exact request decoding and route-before-dispatch
boundary. The SYS2 version and service must match. `Sender::reply` validates
the reply grant against the actual Clone sender, including owner, lender,
active status and the exact non-distributable Push authority. The handler gets
the authenticated caller and decoded operation body.

Only receive Busy conditions before dispatch can restart reception. A retained
payload with Open Busy is retried without waiting for another queued frame.
One absolute `Wait` bounds this reception. The handler runs at most once. A
reply encoding or send failure is reported after possible service side effects.
It never causes automatic redispatch or cancellation.

Call `finish` before the queue owner returns its root. Failed cleanup retains
the watch or delivery for retry. Returning the root is a separate action that
withdraws clients' queue grants. Ingress does not infer resource reclamation
from service completion or a successful send.

Multiplexers using Poll must check `needs_retry` after NotReady: a retained
payload with Open/Return Busy needs another receive attempt without a new queue
event. No request handler has run for that pending receive.

This module and the production Identity multi-endpoint loop are directly
compiled and exercised by `tests/guest/client`. The other old application loops
and supervisor wiring still require porting before assembled Server use.
`system-protocol` is the workspace API v2 dependency name during that port;
unmigrated service code still pins API v1.
