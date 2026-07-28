import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type Config = {
  relay_url: string;
  enrollment_token: string;
  engine: string;
  engine_url: string;
  engine_api_key: string;
  input_price: string;
  output_price: string;
  monthly_earnings_cap: string;
  commercial_hosting_confirmed: boolean;
  model_license: string;
  mode: "static" | "dynamic";
};

type Model = { id: string; architecture: string; context_length: number; quantization?: string };
type ManagedVllmStatus = {
  installed: boolean;
  running: boolean;
  container: string;
  engine_url?: string;
};

const fallback: Config = {
  relay_url: "wss://api.tokens2tokens.com/v1/nodes/connect",
  enrollment_token: "",
  engine: "ollama",
  engine_url: "http://127.0.0.1:11434",
  engine_api_key: "",
  input_price: "0.20",
  output_price: "0.80",
  monthly_earnings_cap: "50000",
  commercial_hosting_confirmed: false,
  model_license: "unknown",
  mode: "static"
};

function App() {
  const [config, setConfig] = useState<Config>(fallback);
  const [models, setModels] = useState<Model[]>([]);
  const [status, setStatus] = useState("Offline");
  const [busy, setBusy] = useState(false);
  const [managedBusy, setManagedBusy] = useState(false);
  const [managedModel, setManagedModel] = useState("Qwen/Qwen2.5-0.5B-Instruct");
  const [managedPort, setManagedPort] = useState(18000);
  const [managedCpu, setManagedCpu] = useState(false);
  const [managedContext, setManagedContext] = useState(1024);
  const [managed, setManaged] = useState<ManagedVllmStatus | null>(null);

  useEffect(() => {
    invoke<Config>("load_config").then(setConfig).catch(() => undefined);
    invoke<ManagedVllmStatus>("managed_vllm_status", { port: 18000 }).then(setManaged).catch(() => undefined);
  }, []);

  const patch = (value: Partial<Config>) => setConfig((current) => ({ ...current, ...value }));

  async function save() {
    setBusy(true);
    try {
      await invoke("save_config", { config });
      setStatus("Configuration saved");
    } finally {
      setBusy(false);
    }
  }

  async function discover() {
    setBusy(true);
    setStatus("Checking local engine…");
    try {
      const found = await invoke<Model[]>("discover_models", { config });
      setModels(found);
      setStatus(`${found.length} model${found.length === 1 ? "" : "s"} ready`);
    } catch (error) {
      setStatus(`Engine unavailable: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function start() {
    await save();
    try {
      await invoke("start_provider");
      setStatus("Online · accepting requests");
    } catch (error) {
      setStatus(`Could not start: ${String(error)}`);
    }
  }

  async function stop() {
    await invoke("stop_provider");
    setStatus("Offline");
  }

  async function refreshManaged() {
    setManagedBusy(true);
    try {
      const current = await invoke<ManagedVllmStatus>("managed_vllm_status", { port: managedPort });
      setManaged(current);
      setStatus(current.running ? "Managed vLLM ready" : "Managed vLLM stopped");
    } catch (error) {
      setStatus(`Docker unavailable: ${String(error)}`);
    } finally {
      setManagedBusy(false);
    }
  }

  async function startManaged() {
    setManagedBusy(true);
    setStatus("Starting managed vLLM · model download may take several minutes…");
    try {
      const current = await invoke<ManagedVllmStatus>("start_managed_vllm", {
        model: managedModel,
        port: managedPort,
        cpu: managedCpu,
        maxModelLen: managedContext
      });
      setManaged(current);
      patch({ engine: "openai-compatible", engine_url: current.engine_url || `http://127.0.0.1:${managedPort}` });
      setStatus("Managed vLLM ready · detect models to publish");
    } catch (error) {
      setStatus(`Managed vLLM failed: ${String(error)}`);
    } finally {
      setManagedBusy(false);
    }
  }

  async function stopManaged() {
    setManagedBusy(true);
    try {
      const current = await invoke<ManagedVllmStatus>("stop_managed_vllm", { port: managedPort });
      setManaged(current);
      setStatus("Managed vLLM stopped");
    } catch (error) {
      setStatus(`Could not stop managed vLLM: ${String(error)}`);
    } finally {
      setManagedBusy(false);
    }
  }

  return (
    <main>
      <header>
        <div className="brand"><span>T2T</span> Token2Token</div>
        <div className={`status ${status.startsWith("Online") ? "online" : ""}`}>{status}</div>
      </header>
      <section className="hero">
        <p className="eyebrow">PROVIDER CONSOLE</p>
        <h1>Turn idle compute<br />into useful credit.</h1>
        <p>Share GPU. Earn Indigo. Run any model.</p>
      </section>
      <section className="grid">
        <article>
          <div className="section-title"><b>01</b> Engine</div>
          <label>Runtime<select value={config.engine} onChange={(e) => {
            const engine = e.target.value;
            patch({ engine, engine_url: engine === "ollama" ? "http://127.0.0.1:11434" : config.engine_url });
          }}>
            <option value="ollama">Ollama</option>
            <option value="openai-compatible">OpenAI-compatible (local or upstream)</option>
          </select></label>
          <label>Local URL<input value={config.engine_url} onChange={(e) => patch({ engine_url: e.target.value })} /></label>
          {config.engine === "openai-compatible" && <label>API key (optional for local engines)<input type="password" value={config.engine_api_key} onChange={(e) => patch({ engine_api_key: e.target.value })} placeholder="Stored only on this machine" /></label>}
          {config.engine === "openai-compatible" && <div className="presets"><button className="ghost" onClick={() => patch({ engine_url: "https://api.deepseek.com" })}>DeepSeek API</button><button className="ghost" onClick={() => patch({ engine_url: "https://api.moonshot.cn" })}>Kimi API</button></div>}
          <button className="secondary" onClick={discover} disabled={busy}>Detect models</button>
          <div className="models">
            {models.map((model) => <div className="model" key={model.id}><strong>{model.id}</strong><small>{model.architecture} · {model.context_length.toLocaleString()} ctx</small></div>)}
            {!models.length && <p className="muted">No models scanned yet.</p>}
          </div>
          <div className="managed">
            <div className="managed-head"><div><b>Managed vLLM</b><small>Docker · local-only endpoint</small></div><span className={managed?.running ? "runtime-on" : ""}>{managed?.running ? "Running" : "Stopped"}</span></div>
            <label>Hugging Face model<input value={managedModel} onChange={(e) => setManagedModel(e.target.value)} /></label>
            <div className="runtime-grid">
              <label>Device<select value={managedCpu ? "cpu" : "gpu"} onChange={(e) => setManagedCpu(e.target.value === "cpu")}><option value="gpu">NVIDIA GPU</option><option value="cpu">CPU fallback</option></select></label>
              <label>Port<input type="number" min="1024" max="65535" value={managedPort} onChange={(e) => setManagedPort(Number(e.target.value))} /></label>
              <label>Max context<input type="number" min="128" max="131072" value={managedContext} onChange={(e) => setManagedContext(Number(e.target.value))} /></label>
            </div>
            <p className="runtime-note">Managed isolation is not confidential computing. Hardware owners may still inspect memory unless the node passes platform attestation.</p>
            <div className="runtime-actions"><button onClick={startManaged} disabled={managedBusy || managed?.running}>Start runtime</button><button className="secondary" onClick={stopManaged} disabled={managedBusy || !managed?.installed}>Stop</button><button className="ghost" onClick={refreshManaged} disabled={managedBusy}>Check</button></div>
          </div>
        </article>
        <article>
          <div className="section-title"><b>02</b> Marketplace</div>
          <div className="prices">
            <label>Input / 1M<input value={config.input_price} onChange={(e) => patch({ input_price: e.target.value })} /></label>
            <label>Output / 1M<input value={config.output_price} onChange={(e) => patch({ output_price: e.target.value })} /></label>
          </div>
          <label>Monthly Indigo cap<input value={config.monthly_earnings_cap} onChange={(e) => patch({ monthly_earnings_cap: e.target.value })} /></label>
          <label>Model mode<select value={config.mode} onChange={(e) => patch({ mode: e.target.value as Config["mode"] })}>
            <option value="static">Static — loaded models only</option>
            <option value="dynamic">Dynamic — switch on demand</option>
          </select></label>
          <label>License identifier<input value={config.model_license} onChange={(e) => patch({ model_license: e.target.value })} /></label>
        </article>
        <article className="wide">
          <div className="section-title"><b>03</b> Connect</div>
          <div className="connect-row">
            <label>Enrollment token<input type="password" value={config.enrollment_token} placeholder="Paste token from provider dashboard" onChange={(e) => patch({ enrollment_token: e.target.value })} /></label>
            <label>Relay URL<input value={config.relay_url} onChange={(e) => patch({ relay_url: e.target.value })} /></label>
          </div>
          <label className="check"><input type="checkbox" checked={config.commercial_hosting_confirmed} onChange={(e) => patch({ commercial_hosting_confirmed: e.target.checked })} /> I confirm the selected models permit commercial hosted inference.</label>
          <div className="actions">
            <button onClick={start} disabled={busy || !config.commercial_hosting_confirmed || !config.enrollment_token}>Start providing</button>
            <button className="secondary" onClick={stop}>Stop</button>
            <button className="ghost" onClick={save} disabled={busy}>Save</button>
          </div>
        </article>
      </section>
      <footer>5% protocol fee · Provider earnings settle in Indigo · Never expose private prompts to untrusted engines</footer>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
