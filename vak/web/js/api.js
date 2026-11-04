// Thin client for the Vak JSON API and its live event stream.
import { t } from "./i18n.js";

export class ApiError extends Error {
  constructor(message, status = 0, code = "") {
    super(message);
    this.status = status;
    this.code = code;
  }
}

async function request(path, { method = "GET", json, form } = {}) {
  const opts = { method, headers: {} };
  if (json !== undefined) {
    opts.headers["Content-Type"] = "application/json";
    opts.body = JSON.stringify(json);
  }
  if (form) opts.body = form;
  let res;
  try {
    res = await fetch(path, opts);
  } catch {
    throw new ApiError(t("err.network"), 0, "network");
  }
  if (res.status === 204) return null;
  const isJson = (res.headers.get("content-type") || "").includes("json");
  const data = isJson ? await res.json() : await res.text();
  if (!res.ok) {
    const detail = data?.detail;
    const message =
      data?.error?.message ||
      (typeof detail === "string" ? detail : Array.isArray(detail) ? detail.map((d) => d.msg).join("; ") : "") ||
      t("err.generic");
    throw new ApiError(message, res.status, data?.error?.code || "");
  }
  return data;
}

export const api = {
  config: () => request("/api/config"),
  budget: () => request("/api/budget"),
  history: () => request("/api/history"),
  plan: (text, overrides) => request("/api/plan", { method: "POST", json: { text, overrides } }),
  estimate: (plan, voice) => request("/api/estimate", { method: "POST", json: { plan, voice } }),
  start: (body) => request("/api/audits", { method: "POST", json: body }),
  audit: (id) => request(`/api/audits/${id}`),
  remove: (id) => request(`/api/audits/${id}`, { method: "DELETE" }),
  cancel: (id) => request(`/api/audits/${id}/cancel`, { method: "POST" }),
  explain: (id, language, force = false) => request(`/api/audits/${id}/explain`, { method: "POST", json: { language, force } }),
  speak: (id, body) => request(`/api/audits/${id}/speak`, { method: "POST", json: body }),
  transcribe(blob, language = "auto") {
    const form = new FormData();
    form.append("audio", blob, "clip.wav");
    form.append("language", language);
    return request("/api/transcribe", { method: "POST", form });
  },
};

const EVENTS = [
  "queued", "phase", "plan", "baseline", "answer", "progress", "test", "units",
  "report", "stage", "explanation", "audio", "notice", "done", "failure", "cancelled",
];
const TERMINAL = new Set(["done", "failure", "cancelled"]);

// Subscribe to an audit's events. Returns a function that closes the stream.
// The browser reconnects by itself and sends Last-Event-ID, so a dropped connection just resumes.
export function listen(id, handlers) {
  const source = new EventSource(`/api/audits/${id}/events`);
  for (const name of EVENTS) {
    source.addEventListener(name, (event) => {
      let data = {};
      try { data = JSON.parse(event.data); } catch { /* an empty payload is fine */ }
      handlers[name]?.(data);
      if (TERMINAL.has(name)) source.close();
    });
  }
  source.onerror = () => {
    if (source.readyState === EventSource.CLOSED) handlers.lost?.();
  };
  return () => source.close();
}
