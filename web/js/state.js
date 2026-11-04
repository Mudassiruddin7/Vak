// Everything the page remembers between visits lives in localStorage, and the page works without it.
const KEY = "vak.v2";
const fresh = () => ({ uiLang: "", theme: "", ownKey: "", session: null, xp: 0, streak: 0, lastDay: "", done: {}, ack: {} });
let data = fresh();
const subs = new Set();

try { data = { ...fresh(), ...JSON.parse(localStorage.getItem(KEY) || "{}") }; } catch { /* private window: start fresh */ }

export const get = () => data;
export function set(patch) {
  data = { ...data, ...patch };
  try { localStorage.setItem(KEY, JSON.stringify(data)); } catch { /* storage blocked: keep it in memory */ }
  subs.forEach((fn) => fn(data));
}
export const subscribe = (fn) => (subs.add(fn), () => subs.delete(fn));

const day = () => new Date().toISOString().slice(0, 10);
/** Adds XP and keeps the daily streak honest: a missed day resets it. */
export function earn(xp) {
  const today = day();
  let { streak, lastDay } = data;
  if (lastDay !== today) {
    const yesterday = new Date(Date.now() - 864e5).toISOString().slice(0, 10);
    streak = lastDay === yesterday ? streak + 1 : 1;
  }
  set({ xp: data.xp + xp, streak, lastDay: today });
}
