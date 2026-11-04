import { get } from "./state.js";
import { token } from "./auth.js";

export class ApiError extends Error {
  constructor(message, status = 0) { super(message); this.status = status; }
}

async function headers(extra = {}) {
  const h = { ...extra };
  const { ownKey } = get();
  if (ownKey) h["x-sarvam-key"] = ownKey;
  const jwt = await token();
  if (jwt) h.authorization = `Bearer ${jwt}`;
  return h;
}

async function fail(res) {
  let msg = "";
  try { msg = (await res.json()).error || ""; } catch { /* not JSON */ }
  throw new ApiError(msg || `The server answered ${res.status}.`, res.status);
}

export async function api(path, { method = "GET", body, form, signal } = {}) {
  let res;
  try {
    res = await fetch(path, {
      method, signal,
      headers: await headers(body ? { "content-type": "application/json" } : {}),
      body: form || (body ? JSON.stringify(body) : undefined),
    });
  } catch (e) {
    if (e.name === "AbortError") throw e;
    throw new ApiError("network", 0);
  }
  if (!res.ok) await fail(res);
  return res.json();
}

/** POST that answers with server-sent events. Calls onEvent(name, data) for each one. */
export async function stream(path, body, onEvent, signal) {
  let res;
  try {
    res = await fetch(path, { method: "POST", signal, headers: await headers({ "content-type": "application/json" }), body: JSON.stringify(body) });
  } catch (e) {
    if (e.name === "AbortError") throw e;
    throw new ApiError("network", 0);
  }
  if (!res.ok) await fail(res);
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    let cut;
    while ((cut = buf.indexOf("\n\n")) >= 0) {
      const block = buf.slice(0, cut);
      buf = buf.slice(cut + 2);
      const name = /^event: (.+)$/m.exec(block)?.[1];
      const data = /^data: (.*)$/m.exec(block)?.[1];
      if (name && data) { try { onEvent(name, JSON.parse(data)); } catch { /* skip a damaged event */ } }
    }
  }
}

export async function speak(text, lang, gender) {
  let res;
  try {
    res = await fetch("/api/speak", { method: "POST", headers: await headers({ "content-type": "application/json" }), body: JSON.stringify({ text, lang, gender }) });
  } catch { throw new ApiError("network", 0); }
  if (!res.ok) await fail(res);
  return URL.createObjectURL(await res.blob());
}
