import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { APPS, markDataUrl, type SmithApp } from "./apps";

type Size = "sm" | "md" | "lg";

export function SmButton(props: {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: Size;
  icon?: boolean;
  disabled?: boolean;
  title?: string;
  type?: "button" | "submit";
  onClick?: (e: MouseEvent) => void;
  children: JSX.Element;
}) {
  const cls = () =>
    ["sm-btn", props.variant && props.variant !== "secondary" ? `sm-btn--${props.variant}` : "", props.size && props.size !== "md" ? `sm-btn--${props.size}` : "", props.icon ? "sm-btn--icon" : ""]
      .filter(Boolean)
      .join(" ");
  return (
    <button type={props.type ?? "button"} class={cls()} disabled={props.disabled} title={props.title} aria-label={props.icon ? props.title : undefined} onClick={(e) => props.onClick?.(e)}>
      {props.children}
    </button>
  );
}

export function Wordmark(props: { app: SmithApp }) {
  const a = () => APPS[props.app];
  return (
    <span class="sm-wordmark">
      <img src={markDataUrl(props.app)} alt="" />
      <span><span class="wm-app">{a().prefix}</span><span class="wm-smith">Smith</span></span>
    </span>
  );
}

export function Segmented<T extends string>(props: { options: { value: T; label: string }[]; value: T; onChange: (v: T) => void; label: string }) {
  return (
    <div class="sm-seg" role="group" aria-label={props.label}>
      <For each={props.options}>
        {(o) => (
          <button type="button" aria-pressed={o.value === props.value} onClick={() => props.onChange(o.value)}>
            {o.label}
          </button>
        )}
      </For>
    </div>
  );
}

export function EmptyState(props: { glyph: string; title: string; body: string; action?: JSX.Element }) {
  return (
    <div class="sm-empty">
      <div class="sm-empty__art" aria-hidden="true">{props.glyph}</div>
      <h3>{props.title}</h3>
      <p>{props.body}</p>
      <Show when={props.action}>{props.action}</Show>
    </div>
  );
}

export function Progress(props: { value: number; label: string }) {
  const pct = () => Math.round(Math.min(1, Math.max(0, props.value)) * 100);
  return (
    <div class="sm-progress" role="progressbar" aria-label={props.label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct()}>
      <i style={{ width: `${pct()}%` }} />
    </div>
  );
}

export function Kbd(props: { children: string }) {
  return <kbd class="sm-kbd">{props.children}</kbd>;
}
