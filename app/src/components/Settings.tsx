import { createResource, createSignal, For, Show } from "solid-js";
import { PROVIDER_DEFAULTS, type Platform, type ProviderKind, type ProviderView } from "@mediasmith/platform";

/** Settings > AI providers. Keys are sent once to the Rust side, which keeps
 *  them in the OS keychain; the UI only ever sees `has_key`. */
export function Settings(props: { platform: Platform }) {
  const ai = props.platform.ai;
  const [view, { mutate }] = createResource(() => ai.settings());
  const [kind, setKind] = createSignal<ProviderKind>("openai");
  const [status, setStatus] = createSignal<Record<string, { ok: boolean; text: string }>>({});
  const say = (id: string, ok: boolean, text: string) => setStatus({ ...status(), [id]: { ok, text } });

  async function add() {
    const d = PROVIDER_DEFAULTS[kind()];
    const taken = new Set(view()?.providers.map((p) => p.id));
    let id: string = kind();
    for (let n = 2; taken.has(id); n++) id = `${kind()}-${n}`;
    try { mutate(await ai.saveProvider({ id, kind: kind(), label: d.label, base_url: d.base_url, model: d.model }, undefined, false)); }
    catch (e) { say("_", false, String(e)); }
  }

  return (
    <div class="settings">
      <div class="sec">AI providers · bring your own key</div>
      <Show when={ai.available} fallback={<p class="note">AI runs in the desktop app for now. Keys stay in the OS keychain there.</p>}>
        <p class="dim">Keys are stored in the OS keychain, never in a file. Requests go straight from this app to the provider.</p>
        <For each={view()?.providers ?? []}>{(p) => <ProviderCard p={p} isDefault={view()?.text_provider === p.id} platform={props.platform}
          onView={mutate} status={status()[p.id]} say={(ok, t) => say(p.id, ok, t)} />}</For>
        <div class="row">
          <select value={kind()} onChange={(e) => setKind(e.currentTarget.value as ProviderKind)}>
            <For each={Object.entries(PROVIDER_DEFAULTS)}>{([k, d]) => <option value={k}>{d.label}</option>}</For>
          </select>
          <button class="btn2" onClick={add}>+ Add</button>
        </div>
        <Show when={status()["_"]}>{(s) => <p class="err">{s().text}</p>}</Show>
      </Show>
    </div>
  );
}

function ProviderCard(props: {
  p: ProviderView; isDefault: boolean; platform: Platform; status?: { ok: boolean; text: string };
  onView: (v: Awaited<ReturnType<Platform["ai"]["settings"]>>) => void; say: (ok: boolean, text: string) => void;
}) {
  const ai = props.platform.ai;
  const [key, setKey] = createSignal("");
  const [base, setBase] = createSignal(props.p.base_url);
  const [model, setModel] = createSignal(props.p.model);
  const needsKey = () => PROVIDER_DEFAULTS[props.p.kind].needsKey;
  const cfg = () => ({ id: props.p.id, kind: props.p.kind, label: props.p.label, base_url: base(), model: model() });

  async function save(makeDefault = props.isDefault) {
    try {
      props.onView(await ai.saveProvider(cfg(), key() ? key() : undefined, makeDefault));
      setKey("");
      props.say(true, "Saved");
    } catch (e) { props.say(false, String(e)); }
  }
  async function test() {
    try { await save(); const m = await ai.testProvider(props.p.id); props.say(true, `Connected · ${m.length} models`); }
    catch (e) { props.say(false, String(e)); }
  }

  return (
    <div class="prov-card">
      <div class="top"><b>{props.p.label}</b>
        <span class={props.p.has_key || !needsKey() ? "ok" : "dim"}>{props.p.has_key ? "key in keychain" : needsKey() ? "no key" : "no key needed"}</span></div>
      <input value={base()} onChange={(e) => setBase(e.currentTarget.value)} placeholder="Base URL" />
      <input value={model()} onChange={(e) => setModel(e.currentTarget.value)} placeholder="Model" />
      <input type="password" value={key()} onInput={(e) => setKey(e.currentTarget.value)}
        placeholder={props.p.has_key ? "•••••• (leave blank to keep)" : needsKey() ? "API key" : "API key (optional)"} />
      <div class="row">
        <button class="btn2" onClick={() => save()}>Save</button>
        <button class="btn2" onClick={test}>Test</button>
        <button class="btn2" disabled={props.isDefault} onClick={() => save(true)}>{props.isDefault ? "Default" : "Use"}</button>
        <button class="btn2" onClick={async () => props.onView(await ai.deleteProvider(props.p.id))}>✕</button>
      </div>
      <Show when={props.status}>{(s) => <p class={s().ok ? "ok" : "err"}>{s().text}</p>}</Show>
    </div>
  );
}
