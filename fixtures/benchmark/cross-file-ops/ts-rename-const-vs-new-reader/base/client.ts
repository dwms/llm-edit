import { API_URL } from "./config";

export function ping(): Promise<Response> {
  return fetch(API_URL);
}
