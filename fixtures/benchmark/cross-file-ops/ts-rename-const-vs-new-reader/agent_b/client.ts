import { API_URL } from "./config";

export function ping(): Promise<Response> {
  return fetch(API_URL);
}

export function healthUrl(): string {
  return `${API_URL}/health`;
}
