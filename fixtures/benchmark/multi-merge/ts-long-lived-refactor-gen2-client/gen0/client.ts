export function httpGet(url: string): Promise<Response> {
  return fetch(url);
}

export function httpPost(url: string, body: unknown): Promise<Response> {
  return fetch(url, { method: "POST", body: JSON.stringify(body) });
}
