# Token2Token Client

**Share GPU. Earn Indigo. Run any model.**

Token2Token Client is the open-source provider software for the Token2Token
inference network. It connects idle GPUs and local model engines to a unified
marketplace while giving providers control over models, prices, resource use,
and monthly Indigo earnings.

## Planned capabilities

- Connect Ollama, LM Studio, llama.cpp, and vLLM.
- Discover locally installed models and stage them for automated conformance tests.
- Configure input and output price per million tokens.
- Select static ready-model or dynamic approved-model-pool operation.
- Set a monthly Indigo earnings limit.
- Connect outbound over an authenticated encrypted relay; no inbound router port.
- Report minimal health, capacity, engine, model, and latency telemetry.
- Run supported managed engines in restricted processes or containers.

## Privacy boundary

Ordinary community hardware is not confidential compute. TLS protects traffic in
transit and managed runtimes reduce accidental exposure, but a machine owner may
still access plaintext in host memory. Do not send sensitive data to Community
providers. A future confidential tier will require compatible TEE hardware,
confidential VMs, and verified remote attestation.

## Repository layout

- `apps/desktop` — Tauri desktop interface.
- `crates/daemon` — background service and lifecycle management.
- `crates/connectors` — local engine adapters.
- `crates/protocol` — generated and hand-written relay protocol bindings.
- `engines/vllm` — managed Linux/NVIDIA vLLM packaging.
- `docs` — provider and security documentation.

Public provider contracts live in the
[Token2Token Provider Specification](https://github.com/luke2023/token2token-provider-spec).
The hosted platform is maintained separately.

## Status

Bootstrap scaffold only. Interfaces will be implemented against versioned public
specifications and conformance tests.

## License

Apache License 2.0. See `LICENSE` and `NOTICE`.
