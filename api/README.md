# System API v2

This crate owns System service payloads and their bounded Weave codecs. The
production dependency graph contains Weave and Kernel API v9. Kernel Client
calls, endpoint lifetime and authorization decisions belong to consumers.

## Requests and replies

Each service Call associates a versioned request with the interface's tagged
response. The request header is little-endian and precedes the operation code.

| Bytes | Field |
| --- | --- |
| 0–3 | ASCII SYS2 |
| 4–5 | Protocol version 2 |
| 6–7 | Service identifier |
| 8–15 | Recipient-local one-shot reply Pend |
| 16 onward | Weave operation tag and typed payload |

| Service identifier | Contract |
| --- | --- |
| 1 | Control lifecycle, supervision observation and recovery |
| 2 | Identity queries, binding and installation |
| 3 | Operator discovery, references and watches |
| 4 | Loader claim, construction, inspection and cancellation |
| 5 | Supply lease acquisition, inspection, release and cancellation |
| 6 | Operator mutation and authority distribution |
| 7 | Retirement request and explicit settlement receipt |
| 8 | Trusted Operator bootstrap wiring |
| 9 | Trusted Identity export |

Bootstrap Frame carries version, provider and System identities. Its
`valid_from` method compares the provider to the transport-authenticated
creator and sender. Consumers must perform that check before adopting endpoints.

The requester must grant the reply Pend before sending its coordinate.
A serialized TaskId describes an identity. A serialized PendId describes a
coordinate in the recipient's table. Neither provides authority by itself.
The receiver authenticates the actual sender through Twine metadata and
checks the supplied route using the corresponding reply-source mechanism.
A new one-shot reply endpoint identifies each call. Use
`request_reply::call::<service::Call>` with the versioned Request, then apply
the selected operation's `Operation::unwrap_response` to check its reply
variant. The generated operations Interface describes the payload union.
Timeout ends waiting, while the server may already have executed an operation.

All consumers must call `decode_exact` at complete-frame boundaries. Unknown
operation and failure codes, truncation, trailing bytes and wrong service or
version are rejected. An unsuccessful encoder can leave partial output.

## Validation and authority

Names contain at most 31 UTF-8 bytes. Paths are canonical and contain at most
eight names and 255 bytes. Argument arrays, identity pages, active coalitions,
namespace lists and descriptions have explicit encoded bounds.

Identity decoding retains authority in each principal and coalition reference.
Active coalition sets are unique, ordered and single-authority. Pagination
cursors identify the target and revision, and their threshold must equal the
last returned slot. Root namespace references and entry zero are distinct.

Transaction IDs reserve zero. Supply lease allocation advances without
wrapping. Kernel rights use the checked v9 Limits representation, including
operation family, Modifiable, Distributable and relative range. Services must
check live grants and permitted attenuation before acting on those values.

Operator grants keep source Task and source Mark constraints independent.
Delegation can introduce either restriction, and descendants must retain each
restriction already present. A grant's optional parent identifies an issued
parent grant; absence is accepted only at the controller's trusted root
entrance. Root bootstrap has no synthetic `GrantId(0)`. Relocating an anchor
checks remaining scope against canonical component boundaries and path bounds.
Namespace Associate carries Mark, Name and Entry and returns a unit receipt;
it is distinct from importing a resource through Bind. A delivered GrantStatus
carries its entrance coordinate in the declared holder's table. The requester
must not interpret it as a local authorization handle.

Control grant mapping keeps Observe, Recover, Ruin and ordinary State distinct.
Identity mutation retains the Install grant boundary. Trusted export has a
separate service identifier and endpoint. Its receiver must authenticate the
bootstrap authority before importing the named binding.
A retirement receipt declares Settled only when the provider has established
its own reclamation conditions.

## Migration and verification

Version 0.2 replaces the old API's serialization and endpoint metadata
generation. Its bytes and Rust API require coordinated Client and Server
migration. Existing Unit schema 1 permissions require explicit translation
into current Kernel rights. The old System runtime remains pinned to its
previous API until its migration is implemented and verified.

```sh
cargo +nightly test --locked -p system-api --tests --target x86_64-unknown-linux-gnu
cargo +nightly clippy --locked -p system-api --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
cargo +nightly build --locked -p system-api --target riscv64gc-unknown-none-elf
cargo +nightly tree --locked -p system-api@0.2.0 -e normal,build
```

These checks establish codec and target compilation behavior. End-to-end
System execution and lifecycle verification are tracked in System issue 10.
