import { goto } from '$app/navigation';
import { browser } from '$app/environment';

const CSRF_KEY = 'objexel_csrf';

export function setCsrf(token: string): void {
  if (!browser) return;
  try { sessionStorage.setItem(CSRF_KEY, token); } catch { /* storage unavailable */ }
}

export function clearCsrf(): void {
  if (!browser) return;
  try { sessionStorage.removeItem(CSRF_KEY); } catch { /* storage unavailable */ }
}

function cookieValue(name: string): string | null {
  if (!browser) return null;
  const prefix = `${name}=`;
  for (const part of document.cookie.split(';')) {
    const trimmed = part.trim();
    if (trimmed.startsWith(prefix)) return decodeURIComponent(trimmed.slice(prefix.length));
  }
  return null;
}

function csrfToken(): string | null {
  if (!browser) return null;
  // The server issues a readable objexel_csrf cookie that shares the session's lifetime,
  // so it survives across tabs and reloads. Fall back to sessionStorage from the login turn.
  const fromCookie = cookieValue(CSRF_KEY);
  if (fromCookie) return fromCookie;
  try { return sessionStorage.getItem(CSRF_KEY); } catch { return null; }
}

export type ApiOptions = Omit<RequestInit, 'body'> & {
  /** Serialized to JSON and sent as the request body with the correct content-type. */
  json?: unknown;
  body?: BodyInit;
  /** Redirect to /login when the API replies 401. Defaults to true. */
  redirectOnUnauthorized?: boolean;
};

/**
 * Fetch wrapper for the Objexel API. It always sends the session cookie, attaches the
 * CSRF token to mutating requests (the backend requires it for user management and
 * logout), and sends the browser to /login when the session is rejected.
 */
export async function api(path: string, options: ApiOptions = {}): Promise<Response> {
  const { json, redirectOnUnauthorized = true, headers, method: rawMethod, ...rest } = options;
  const method = (rawMethod ?? 'GET').toUpperCase();
  const merged = new Headers(headers);
  const init: RequestInit = { credentials: 'same-origin', ...rest, method };

  if (json !== undefined) {
    merged.set('content-type', 'application/json');
    init.body = JSON.stringify(json);
  } else if (options.body !== undefined) {
    init.body = options.body;
  }

  if (method !== 'GET' && method !== 'HEAD') {
    const token = csrfToken();
    if (token) merged.set('x-csrf-token', token);
  }

  init.headers = merged;
  const response = await fetch(path, init);

  if (response.status === 401 && redirectOnUnauthorized && browser) {
    clearCsrf();
    await goto('/login');
  }
  return response;
}

/**
 * True when the browser holds a working session — i.e. the session cookie was actually
 * stored and is accepted by the API. Used right after login/setup to detect the case where
 * the server issued a `Secure` cookie the browser dropped (plain-HTTP deployments), which
 * otherwise looks like a silent bounce back to /login.
 */
export async function sessionActive(): Promise<boolean> {
  try {
    const response = await api('/api/auth/me', { redirectOnUnauthorized: false });
    return response.ok;
  } catch {
    return false;
  }
}

/** Guidance shown when authentication succeeds but no session cookie was persisted. */
export const SESSION_NOT_STORED_MESSAGE =
  'Signed in, but your browser did not store the session cookie. If you are reaching Objexel over plain HTTP, set OBJEXEL_COOKIE_SECURE=0 in the server .env and run `docker compose up -d` — or serve Objexel over HTTPS.';

/** Extract a human-readable message from an error response (the API returns {"error": "..."}). */
export async function apiError(response: Response): Promise<string> {
  const text = await response.text();
  try {
    const parsed = JSON.parse(text);
    if (parsed && typeof parsed.error === 'string') return parsed.error;
  } catch { /* not JSON */ }
  return text || `Request failed (${response.status})`;
}
