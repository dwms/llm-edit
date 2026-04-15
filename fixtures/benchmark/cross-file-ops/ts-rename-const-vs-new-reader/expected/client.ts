import { BASE_URL } from "./config";

export function ping(): Promise<Response> {
  return fetch(BASE_URL);
}

export function healthUrl(): string {
  return `${BASE_URL}/health`;
}
