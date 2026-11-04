// Small DOM helpers. The important one is `html`: every interpolated value is escaped
// unless it was marked safe with `raw()` or came out of another `html` call. Model output
// and user text therefore can't inject markup.

export const $ = (selector, root = document) => root.querySelector(selector);
export const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];

const ENTITIES = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
export const esc = (value) => String(value ?? "").replace(/[&<>"']/g, (c) => ENTITIES[c]);

const SAFE = Symbol("safe");
export const raw = (markup) => ({ [SAFE]: true, markup: String(markup) });

function part(value) {
  if (value && value[SAFE]) return value.markup;
  if (Array.isArray(value)) return value.map(part).join("");
  return esc(value);
}

export function html(strings, ...values) {
  let out = strings[0];
  values.forEach((value, i) => {
    out += part(value) + strings[i + 1];
  });
  return raw(out);
}

export function mount(el, template) {
  el.innerHTML = template[SAFE] ? template.markup : esc(template);
  return el;
}

export function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === false || v == null) continue;
    if (k === "class") node.className = v;
    else if (k === "dataset") Object.assign(node.dataset, v);
    else if (k.startsWith("on") && typeof v === "function") node.addEventListener(k.slice(2), v);
    else node.setAttribute(k, v === true ? "" : v);
  }
  for (const child of children.flat()) {
    if (child == null || child === false) continue;
    node.append(child.nodeType ? child : document.createTextNode(child));
  }
  return node;
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
export const clamp = (n, lo, hi) => Math.min(hi, Math.max(lo, n));
export const reducedMotion = () =>
  document.documentElement.hasAttribute("data-still") || window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function debounce(fn, ms) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

export function formatInr(value) {
  const n = Number(value) || 0;
  if (n >= 100) return "₹" + Math.round(n).toLocaleString("en-IN");
  if (n >= 10) return "₹" + n.toFixed(1);
  return "₹" + n.toFixed(2);
}

export function timeAgo(ts, lang) {
  const secs = Math.max(1, Math.round(Date.now() / 1000 - ts));
  const rtf = new Intl.RelativeTimeFormat(lang === "en" ? "en" : lang === "te" ? "te" : "hi", { numeric: "auto" });
  if (secs < 90) return rtf.format(-secs, "second");
  if (secs < 5400) return rtf.format(-Math.round(secs / 60), "minute");
  if (secs < 129600) return rtf.format(-Math.round(secs / 3600), "hour");
  return rtf.format(-Math.round(secs / 86400), "day");
}
