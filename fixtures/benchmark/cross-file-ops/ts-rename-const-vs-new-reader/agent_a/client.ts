import { BASE_URL } from "./config";

export function ping(): Promise<Response> {
  return fetch(BASE_URL);
}
