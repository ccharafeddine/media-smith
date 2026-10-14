import type { JSX } from "solid-js";

export function Button(props: { variant?: "primary" | "ghost"; children: JSX.Element; onClick?: () => void; title?: string }) {
  return (
    <button class={`ms-btn ms-btn--${props.variant ?? "ghost"}`} title={props.title} onClick={() => props.onClick?.()}>
      {props.children}
    </button>
  );
}
