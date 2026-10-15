// Whisper in a Web Worker (transformers.js on onnxruntime-web): the browser
// auto-captions, standing in for the desktop whisper-cli sidecar. Everything is same-origin:
// the ort wasm is a hashed build asset and the model at /models/, with files over the
// 25 MiB static-asset cap split into parts and joined here.
import ortWasm from "../../../node_modules/@huggingface/transformers/dist/ort-wasm-simd-threaded.jsep.wasm?url";
import { env, pipeline, WhisperTextStreamer, type AutomaticSpeechRecognitionPipeline } from "@huggingface/transformers";

const MODEL = "whisper-base_timestamped";
env.allowRemoteModels = false;
env.allowLocalModels = true;
env.useBrowserCache = false;
env.useCustomCache = true;
env.customCache = {
  async match(key: string) {
    if (typeof key !== "string" || !key.endsWith(".onnx")) return undefined;
    const r = await fetch(key + ".parts");
    if (!r.ok) return undefined;
    // A host may answer a missing manifest with the SPA's index.html.
    let m: { parts: string[]; bytes: number } | null = null;
    try { m = JSON.parse(await r.text()); } catch { return undefined; }
    if (!m?.parts?.length) return undefined;
    const base = key.slice(0, key.lastIndexOf("/") + 1);
    const blobs = await Promise.all(m.parts.map(async (p) => {
      const res = await fetch(base + p);
      if (!res.ok) throw new Error(`model part ${p}: HTTP ${res.status}`);
      return res.blob();
    }));
    return new Response(new Blob(blobs), { headers: { "content-length": String(m.bytes) } });
  },
  async put() { /* the HTTP cache keeps the parts */ },
};
const onnx = env.backends.onnx as { wasm?: { wasmPaths?: string | { wasm?: string }; numThreads?: number }; logLevel?: string };
if (onnx.wasm) {
  onnx.wasm.wasmPaths = { wasm: new URL(ortWasm, self.location.href).href };
  // ort pthreads would re-load this bundled worker as their script and stall.
  onnx.wasm.numThreads = 1;
}
onnx.logLevel = "error";

let asr: Promise<AutomaticSpeechRecognitionPipeline> | null = null;
function load(post: (m: unknown) => void) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  asr ??= (pipeline as any)("automatic-speech-recognition", MODEL, {
    dtype: { encoder_model: "q8", decoder_model_merged: "q8" },
    device: "wasm",
    progress_callback: (p: { status: string; progress?: number; file?: string }) => {
      if (p.status === "progress" && p.file?.endsWith(".onnx")) post({ type: "load", percent: p.progress ?? null });
    },
  }) as Promise<AutomaticSpeechRecognitionPipeline>;
  asr.catch(() => (asr = null));
  return asr;
}

type Req = { modelRoot: string; audio: Float32Array; language: string | null; translate: boolean; words: boolean };
type Chunk = { text: string; timestamp: [number, number | null] };

self.onmessage = async (ev: MessageEvent<Req>) => {
  const post = (m: unknown) => (self as unknown as Worker).postMessage(m);
  try {
    const { modelRoot, audio, language, translate, words } = ev.data;
    env.localModelPath = modelRoot;
    const p = await load(post);
    const secs = audio.length / 16000;
    // The pipeline walks 30 s windows that advance by 20 s (5 s stride each side).
    const total = Math.max(1, Math.ceil(Math.max(0, secs - 10) / 20));
    let done = 0;
    const streamer = new WhisperTextStreamer(p.tokenizer as ConstructorParameters<typeof WhisperTextStreamer>[0], {
      skip_prompt: true,
      on_chunk_end: () => {},
      on_finalize: () => { done++; post({ type: "progress", percent: Math.min(99, (done / total) * 100) }); },
    });
    post({ type: "progress", percent: 0 });
    const out = (await p(audio, {
      chunk_length_s: 30,
      stride_length_s: 5,
      return_timestamps: words ? "word" : true,
      language: language && language !== "auto" ? language : undefined,
      task: translate ? "translate" : "transcribe",
      streamer,
    })) as { text: string; chunks?: Chunk[] };
    post({ type: "done", chunks: out.chunks ?? [], secs });
  } catch (e) {
    post({ type: "error", message: e instanceof Error ? e.message : String(e) });
  }
};
