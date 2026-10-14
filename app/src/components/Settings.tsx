import { For } from "solid-js";
import type { Platform } from "@mediasmith/platform";
import { AI_PROVIDERS } from "../mock";

/** Settings > AI providers: UI stub only. Keys are not stored yet; see
 *  docs/ai.md for the keychain-backed design. */
export function Settings(props: { platform: Platform }) {
  return (
    <div class="settings">
      <div class="sec">AI providers · bring your own key</div>
      <p class="note">Coming in phase 4. Keys will live in the {props.platform.kind === "desktop" ? "OS keychain" : "browser (this device only)"}, and requests go straight from this app to the provider.</p>
      <For each={AI_PROVIDERS}>{(p) => (
        <label class="prov">
          <span>{p.label} <em class="dim">{p.defaultModel}</em></span>
          <input type="password" placeholder={p.hint} disabled />
        </label>
      )}</For>
    </div>
  );
}
