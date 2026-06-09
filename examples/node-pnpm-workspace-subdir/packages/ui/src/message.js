import clsx from "clsx";

export const messageClass = clsx("workspace-card", "ready");

export function renderMessage(name) {
  return `Hello from ${name}`;
}
