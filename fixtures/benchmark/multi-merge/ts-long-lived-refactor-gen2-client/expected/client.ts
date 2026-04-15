export function get(url: string): Promise<Response> {
  return fetch(url);
}

export function post(url: string, body: unknown): Promise<Response> {
  return fetch(url, { method: "POST", body: JSON.stringify(body) });
}

export async function httpPut(url: string, body: unknown): Promise<Response> {
  const res = await get(url);
  if (res.ok) return post(url, body);
  return res;
}
