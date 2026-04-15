import { track } from "./api";

export function onClick() {
  track("click", { at: Date.now() });
}

export function onSubmit() {
  track("submit", { at: Date.now() });
}
