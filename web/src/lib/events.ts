import { browser } from '$app/environment';

export type LiveMessage = { kind: string; data?: unknown };
export type LiveHandlers = {
  /** Called for every JSON message from the server (event, observation, zone_event, connected). */
  onMessage: (message: LiveMessage) => void;
  /** Called with the current connection state so the UI can show a live indicator. */
  onStatus?: (connected: boolean) => void;
};

/**
 * Open the live-events WebSocket (/api/v1/events) and stream messages to the handlers.
 * The same-origin handshake carries the session cookie, so the server authenticates it.
 * Reconnects with capped exponential backoff. Returns a disposer to close the socket -
 * call it from onMount's cleanup.
 */
export function subscribeLiveEvents(handlers: LiveHandlers): () => void {
  if (!browser) return () => {};
  let socket: WebSocket | null = null;
  let closed = false;
  let attempts = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const connect = () => {
    if (closed) return;
    const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
    socket = new WebSocket(`${scheme}://${location.host}/api/v1/events`);
    socket.onopen = () => { attempts = 0; handlers.onStatus?.(true); };
    socket.onmessage = (event) => {
      try { handlers.onMessage(JSON.parse(event.data) as LiveMessage); } catch { /* ignore malformed frame */ }
    };
    socket.onclose = () => {
      handlers.onStatus?.(false);
      if (closed) return;
      attempts = Math.min(attempts + 1, 6);
      timer = setTimeout(connect, Math.min(1000 * 2 ** attempts, 30000));
    };
    socket.onerror = () => { socket?.close(); };
  };

  connect();
  return () => { closed = true; if (timer) clearTimeout(timer); socket?.close(); };
}
