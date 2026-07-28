# Desktop application

The Tauri provider console is the GUI companion to the `token2token` CLI. It
supports Windows, Ubuntu, and macOS and exposes:

- Ollama and OpenAI-compatible engine configuration and automatic model discovery.
- Provider prices, static/dynamic model mode, license confirmation, and monthly
  Indigo earnings limits.
- Enrollment and provider process lifecycle controls.
- A managed vLLM panel that starts a local-only Docker runtime on NVIDIA GPU or
  CPU, persists model caches, and configures the connector when it becomes ready.

The release workflow bundles the CLI as a Tauri sidecar. Docker is required only
for the optional managed vLLM runtime. Its CPU image requires x86-64 and GPU
mode requires an NVIDIA Linux/Windows host. Apple Silicon providers can connect
Ollama or LM Studio; unsupported combinations fail with an explicit message.
