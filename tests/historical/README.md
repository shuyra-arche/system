# Historical harness sources

`server-support/` preserves the former in-production callback/test harness and
`loader-service/` preserves the retired Env/Wire/ECS Loader service. Their
current native successors live in the production runtime, Loader, authenticated
Control composition and `tests/guest/` scenarios. These files retain source
history and are not compiled or offered as a supported feature.

The older standalone test packages outside the active README/CI commands still
refer to the retired ECS/SDK model. Their old dependency locks and source-shape
assertions are historical fixtures, not ABI 11 verification. Do not re-enable
the legacy production graph to make them build. Actual ownership assertions
must be ported to the native boundary before claiming the corresponding S1/S2
coverage; the new basic application scenarios do not replace the full recovery,
construction, export, service publication or device-retirement campaigns.
