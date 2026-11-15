# External Supply registration v1

Supply domain 5 adds operations 5–7 to the existing SYS2 v4 contract. Consumer
operations 1–4 keep their encoding and meanings. Select the published System
revision containing this contract for API, Client and Server together; an older
Server cannot serve the new operations. Kernel ABI/Ledger remains 13/13 and all
provider pins and target flags remain unchanged.

## Authority and installation

The provider keeps its original resource. It creates a dedicated offering branch,
grants a descendant to the Task owning the System Supply endpoint and sends that
**System-local** PendId in Register. Keep the local offering branch until the
registration outcome and cleanup are settled. A provider-local token is not an
offer to another Task. A serialized number, resource name or hardware address
does not establish authority.

System checks the actual ingress caller's currently running Instance and native
Team/Task membership. The source key must equal that Unit's trusted declaration
name. Supply initialization is permitted before application Ready; other
Control supervision/construction readiness requirements retain their meaning.
Stopping/frozen-intent/retired Instances cannot acquire fresh provider authority.

At serial admission System inspects the received grant. Its immediate Kernel
`lender` must be the actual caller, its status Active and its actual limits must
permit the declared maximum **with distribution enabled** for private lease
derivation. The grant's object owner need not equal its lender: a provider may
lawfully delegate authority received from another owner. Kernel checks live
authority again during every subsequent derivation/delivery; an observation is
not a durable authorization. Built-in source namespaces are reserved, including resource names not yet registered.

This first registration contract admits Object-scoped Sign and Clone grants.
Flat, Nest, IRQ/External, MMIO, device storage and hardware quiescence are outside
this increment. No address or description manufactures these authorities.

A trusted consumer Unit names the provider Unit and resource in its schema-2
Supply bindings. Existing Control creates the actual target Task; its Instance
Installation derives private roots and delivers the declared narrowed grants
before construction completes. Register does not select a target or authorize
arbitrary Task management. Ordinary Acquire still requires `target == actual
caller` and the running caller's trusted binding policy.

External entries participate only in named Supply lookup. They are not added to
the application's global marked-root publication lookup. A native lease owner
continues to own each installation/consumer lease; the registration registry is
not a second Instance, device authority or lease state machine.

## Operations and commit boundaries

Registration IDs are nonzero and scoped to `(serving System Task, actual caller
Task, RegistrationId)`. They are separate from consumer LeaseIds and Instance
operation IDs. The registry retains at most 64 accepted/cancelled records for its
lifetime, including terminal history. IDs and accepted offered grants are not recycled.
The selected Kernel allocates Pend tokens monotonically without reuse.

| Operation | Wire code | Admission / commitment | Result and retry |
| --- | --- | --- | --- |
| Register | 5 | After replay, policy, capacity and actual grant checks, append the immutable request and accept responsibility for that imported grant. No additional grant is created by registration. | Active; exact replay returns the retained state without reimporting or reinspecting. Changed intent under the same accepted ID is Conflict. Bad/Denied/Full/Pending before commitment accepts no grant. |
| InspectSource | 6 | Actual caller's registration ID only; no mutation or authority transfer. | Active, Closing, Closed or Cancelled; Unknown means no retained record for that caller/ID, not proof that an in-flight Register cannot arrive. |
| Withdraw | 7 | Actual caller's ID; Active becomes Closing and immediately leaves lookup. Repeats retain the same root and state. | Closing until actual withdrawal completes; Closed replays. An unseen ID commits a Cancelled tombstone so a delayed Register cannot reopen it. Full is not a cancellation acknowledgment. |

`Cancelled` means no offered grant was accepted. A late Register under a cancelled
ID stays Cancelled and imports nothing; the provider must explicitly retire any
local offering branch it had already delegated. `Closed` instead means an
accepted imported branch has completed native retirement. These states must not
be treated as interchangeable receipts for an already delegated grant.

A transport timeout or lost reply is an unknown outcome. Retain the original
request, ID, offered System-local token and offering branch. Query/replay that
same request or Withdraw the same ID; do not manufacture another grant for the
same accepted registration. Pending returned before Register commitment can be
retried with the same request under the original deadline. SDK reply cleanup
remains independently owned by Pending/Failure/Cleanup.

## Withdrawal and replacement

Closing blocks fresh installations/lookups. An accepted imported root is never
handed over: native Return success, or strict absence of that same owned local
record after ancestor withdrawal, is the completion evidence for Closed. Busy or
other errors keep the same root in Closing. A stopping/lost provider or observed non-active
grant closes admission and schedules the same retirement; Task observations
alone do not commit Closed. Shutdown retains the registry through Instance and
consumer cleanup, then finishes accepted imported roots explicitly.

An existing consumer route keeps its original lease/source owner. A fresh lease
uses the current resource's actual root and trusted maximum as its cache key;
same-name replacement cannot reactivate an old source cache or old operation.
After Closed, a new registration ID and fresh offering grant may bind the same
source/resource. Old Register/Withdraw replays affect only their old record.
Until Closed, reusing that name/resource or aliasing an accepted offered token is
Conflict. Historical accepted tokens remain reserved even after closure.

Withdrawal removes the registered Pend branch and descendants, while the
provider's original resource stays owned by the provider. It neither stops an
Instance nor establishes Task/Team reclamation, hardware silence or permission
to reuse device storage. Those owners keep their separate retirement evidence.

## Verification

API tests protect bounded frames, nonzero registration IDs, source states and
legacy operation encodings. Server Host tests exercise actual registration owner
decisions with supplied native observations/completions: caller/name/lender,
rights, suspension, replay/conflicts, cancellation-before-register, Busy/error
retention, replacement and capacity exhaustion. These are not native-trap proof.

The `supply` handoff scenario runs the complete production Server and separate
provider/Driver User Teams through public API/Client and actual Kernel calls.
It registers before provider Ready, installs declared Sign grants before Driver
start, checks narrowing and the System lender, exercises ordinary self Acquire,
rejects arbitrary-target Acquire and provider-name/rights escalation, withdraws
while Driver RPC remains usable, re-registers the same name, exercises a fresh
Acquire from the same live Driver, and revokes the provider's offering ancestor.
It preserves original provider authority, cancelled-ID ownership rules, typed
Instance retirement and Kernel-admitted System root exit.

```sh
cargo test --locked -p system-api -p system-client -p system-support --tests --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path server/tests/Cargo.toml --target x86_64-unknown-linux-gnu
python3 tests/guest/system/run.py /path/to/qualified/kernel --handoff --profile release --kernel-profile release --harts 1 2 4 --scenarios supply
```

The default handoff campaign retains the prior seven scenarios and adds supply:
eight scenarios × four Kernel/System profile pairs × 1/2/4 Harts = 96 cases.
Existing role/VM/startup bounds and evidence hashing are unchanged. Standalone
Hub boot/consumer integration, provider Task death with an independently owned
surviving consumer, external Clone-specific campaigns, full pressure/fault
coverage, Toolchain product and physical devices remain separate acceptance.
