import { sendEvent } from "./api";

export function onClick() {
  sendEvent("click", { at: Date.now() });
}
