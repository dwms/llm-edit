export function sendEvent(name, payload) {
  return fetch("/events", { method: "POST", body: JSON.stringify({ name, payload }) });
}
