const API_BASE_URL = process.env.NEXT_PUBLIC_API_BASE_URL || "";

export interface ApiRequestOptions {
  method?: string;
  token?: string | null;
  body?: unknown;
}

export async function apiFetch<T = any>(
  path: string,
  options: ApiRequestOptions = {}
): Promise<T> {
  const { method = "GET", token, body } = options;
  const url = API_BASE_URL ? `${API_BASE_URL}${path}` : path;

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(url, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || `Request failed with status ${res.status}`);
  }

  if (res.status === 204) {
    return undefined as unknown as T;
  }

  return (await res.json()) as T;
}
