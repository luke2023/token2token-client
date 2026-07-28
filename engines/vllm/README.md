# Managed vLLM

Token2Token can run a supported vLLM OpenAI-compatible server as a Docker
container, bound only to `127.0.0.1`. The CLI and desktop app provide the same
lifecycle controls:

```bash
token2token managed-vllm start \
  --model Qwen/Qwen2.5-0.5B-Instruct \
  --port 18000 \
  --max-model-len 1024

token2token managed-vllm status --port 18000
token2token managed-vllm stop
```

Add `--cpu` for the x86-64 CPU image. Model and vLLM caches are stored in named
Docker volumes, while the HTTP port remains loopback-only.

The current GPU image requires a vLLM-supported NVIDIA architecture and NVIDIA
Container Toolkit. Pascal GPUs such as the Quadro P1000 are not supported by
current upstream vLLM and should use Ollama or llama.cpp as ordinary community
nodes.

GPU mode is supported on NVIDIA Linux/Windows hosts; CPU mode requires x86-64.
On Apple Silicon, connect Ollama or LM Studio through the same Token2Token app.

Managed lifecycle and container isolation do **not** provide encryption-in-use.
Token2Token must advertise confidential compute only after a hardware-backed
attestation policy has verified a supported confidential-computing platform.
