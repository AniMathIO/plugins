# Contributing

For a community listing, submit a PR adding a table row with a short description,
public source URL, explicit license, passing public CI URL and minimum ABI version.
The source must build for wasm32-wasip2 and package a matching plugin.toml and
plugin.wasm. Use unique reverse-domain IDs and document parameters and host requirements.
No network access or telemetry beyond what the documented sandbox permits; disclose
all host services and capabilities used. Do not imply that a listing is a security audit.

Template/example changes must pass CI including generated-template builds and
released-host smoke renders. Keep the vendored ABI unchanged unless deliberately
upgrading the contract and documenting compatibility. Use Conventional Commits.
