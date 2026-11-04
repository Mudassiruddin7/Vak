// Sign-in with Supabase, by hand: a few fetch calls, no SDK. Rows are read and written with the person's
// own token, so the database's row-level security decides what they can see.
import { get, set } from "./state.js";

let cfg = null;
export const configure = (c) => { cfg = c; };
export const enabled = () => !!cfg;
export const user = () => get().session?.user || null;

async function auth(path, body) {
  const res = await fetch(`${cfg.url}/auth/v1/${path}`, { method: "POST", headers: { apikey: cfg.anon_key, "content-type": "application/json" }, body: JSON.stringify(body) });
  const j = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(j.msg || j.error_description || j.message || "That didn't work.");
  return j;
}

const keep = (j) => set({ session: { access_token: j.access_token, refresh_token: j.refresh_token, expires_at: Date.now() + (j.expires_in || 3600) * 1000, user: { id: j.user?.id, email: j.user?.email } } });

export async function signUp(email, password) {
  const j = await auth("signup", { email, password });
  if (j.access_token) { keep(j); return "in"; }
  return "confirm";
}
export async function signIn(email, password) { keep(await auth("token?grant_type=password", { email, password })); }
export function signOut() { set({ session: null }); }

/** A valid access token, refreshed when it is about to expire; null when signed out. */
export async function token() {
  const s = get().session;
  if (!s || !cfg) return null;
  if (s.expires_at - Date.now() > 60_000) return s.access_token;
  try { keep(await auth("token?grant_type=refresh_token", { refresh_token: s.refresh_token })); return get().session.access_token; }
  catch { set({ session: null }); return null; }
}

async function rest(path, opts = {}) {
  const jwt = await token();
  if (!cfg || !jwt) throw new Error("signed out");
  const res = await fetch(`${cfg.url}/rest/v1/${path}`, { ...opts, headers: { apikey: cfg.anon_key, authorization: `Bearer ${jwt}`, "content-type": "application/json", ...(opts.headers || {}) } });
  if (!res.ok) throw new Error((await res.json().catch(() => ({}))).message || `HTTP ${res.status}`);
  return res.status === 204 ? null : res.json();
}
export const saveAudit = (row) => rest("audits", { method: "POST", headers: { Prefer: "return=representation" }, body: JSON.stringify(row) });
export const myAudits = () => rest("audits?select=id,prompt,score,grade,created_at,slug&order=created_at.desc&limit=12");
export const saveProgress = (p) => rest("progress?on_conflict=user_id", { method: "POST", headers: { Prefer: "resolution=merge-duplicates" }, body: JSON.stringify(p) });
export const loadProgress = () => rest("progress?select=xp,streak,last_day,done&limit=1");
