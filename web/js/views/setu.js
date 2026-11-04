import { html, mount, el, $, $$ } from "../dom.js";
import { t, getLang } from "../i18n.js";
import { api, speak, ApiError } from "../api.js";
import * as S from "../state.js";
import { startRecording, micSupported } from "../recorder.js";
import { toast, sfx } from "../fx.js";

const msg = (e) => (e instanceof ApiError ? (e.message === "network" ? t("err.network") : e.message) : e.message === "silent" ? t("err.mic") : e.message || t("err.generic"));
const LEVEL = { good: "ok", check: "wobble", confirm: "broke" };

export function setu(host, app) {
  const cfg = app.config;
  const langs = cfg.languages;
  const L = (c) => langs.find((l) => l.code === c);
  const pick = (c, fb) => (L(c) ? c : fb);
  const st = { a: pick(getLang() === "en" ? "hi" : getLang(), "hi"), b: "en", domain: "general", careful: false, turns: [], gender: { a: "", b: "" }, rec: null, busy: false, ack: false };
  if (st.a === "en") st.b = "hi";
  let player = null;
  const dom = () => cfg.domains.find((d) => d.id === st.domain) || cfg.domains[0];

  function opts(sel) {
    return langs.map((l) => html`<option value="${l.code}" ${l.code === sel ? "selected" : ""}>${l.native}${l.english !== l.native ? ` · ${l.english}` : ""}${l.voice ? "" : " ·T"}</option>`);
  }

  function consent() {
    const d = dom();
    if (!d.consent || st.ack) return "";
    const n = (c) => L(c)?.notices || L("en").notices;
    const block = (c) => html`<div class="card" lang="${c}"><span class="eyebrow">${L(c).native}</span><p style="margin-top:.4rem">${n(c).ai_translation} ${n(c).not_advice}</p><p class="hint">${n(c).privacy}</p>
      ${L(c).voice ? html`<button class="btn sm line" type="button" data-say="${c}" style="margin-top:.5rem">▶ ${t("s.play")}</button>` : ""}</div>`;
    return html`<div class="card stack" style="border-color:var(--ink)"><h3 class="h3" style="font-size:1.4rem">${t("s.consent")}</h3><div class="grid2">${block(st.a)}${block(st.b)}</div>
      <button class="btn go" type="button" id="agree">${t("s.agree")}</button></div>`;
  }

  function turnCard(x) {
    const tr = x.turn.trust;
    const issues = tr.issues.map((i) => t("s.i." + i)).join(" · ");
    return html`<article class="card stack" style="margin-${x.side === "a" ? "right" : "left"}:clamp(0rem,8vw,4rem);gap:.5rem" data-turn="${x.id}">
      <div class="row"><span class="eyebrow">${L(x.from).native} → ${L(x.to).native}</span><span class="grow"></span>
        <span class="chip ${LEVEL[tr.level]}" title="${issues}">${t("s." + tr.level)} · ${tr.score}</span></div>
      <p class="muted" lang="${x.from}">${x.turn.source}</p>
      <p class="say" lang="${x.to}">${x.turn.translation}</p>
      ${issues ? html`<p class="hint">⚠ ${issues}</p>` : ""}
      ${x.turn.back_translation ? html`<details><summary class="hint" style="cursor:pointer">${t("s.back")}</summary><p class="muted" lang="${x.from}" style="margin-top:.3rem">${x.turn.back_translation}</p></details>` : ""}
      ${x.turn.needs_confirm && !x.confirmed ? html`<div class="err-box stack" style="gap:.5rem"><b>${t("s.q")}</b><p lang="${x.from}">${x.turn.back_translation || x.turn.source}</p>
        <div class="row"><button class="btn sm" type="button" data-go="${x.id}">${t("s.say")}</button><button class="btn sm line" type="button" data-redo="${x.id}">${t("s.redo")}</button></div></div>`
        : x.turn.can_speak ? html`<div class="row"><button class="btn sm line" type="button" data-play="${x.id}">▶ ${t("s.play")}</button></div>` : html`<p class="hint">${t("s.novoice")}</p>`}
    </article>`;
  }

  function draw() {
    const d = dom(), a = L(st.a), b = L(st.b);
    mount(host, html`
      <div class="bench-head"><div><h2>${t("s.title")}</h2><p class="muted" style="margin-top:.4rem">${t("s.sub")}</p></div></div>
      <div class="stack">
        <div class="grid2" style="align-items:end">
          <label class="field">${t("s.a")}<select class="input" id="la">${opts(st.a)}</select></label>
          <label class="field">${t("s.b")}<select class="input" id="lb">${opts(st.b)}</select></label>
        </div>
        <div class="row"><button class="btn sm line" type="button" id="swap">⇄ ${t("s.swap")}</button>
          <label class="field" style="min-width:14rem">${t("s.domain")}<select class="input" id="dm">${cfg.domains.map((x) => html`<option value="${x.id}" ${x.id === st.domain ? "selected" : ""}>${x.label}</option>`)}</select></label>
          <label class="chip" style="cursor:pointer"><input type="checkbox" id="careful" ${st.careful ? "checked" : ""}> ${t("s.careful")}</label><span class="hint">${t("s.careful.d")}</span></div>
        ${consent()}
        <div class="grid2">
          ${["a", "b"].map((side) => { const l = side === "a" ? a : b; return html`<div class="card stack" style="justify-items:center;text-align:center">
            <span class="eyebrow">${side === "a" ? t("s.a") : t("s.b")}</span><h3 class="h3" style="font-size:1.6rem" lang="${l.code}">${l.native}</h3>
            ${micSupported() ? html`<button class="mic" type="button" data-mic="${side}" aria-label="${t("s.tap")}" ${st.busy || (d.consent && !st.ack) ? "disabled" : ""}><span class="bars"><i></i><i></i><i></i><i></i><i></i></span></button>` : html`<p class="hint">${t("s.nomic")}</p>`}
            <span class="hint" data-note="${side}">${t("s.tap")}</span>
            <form class="row" data-type="${side}" style="width:100%"><input class="input grow" placeholder="${t("s.type")}" lang="${l.code}" maxlength="${cfg.limits.turn_chars}"><button class="btn sm" type="submit" ${st.busy ? "disabled" : ""}>${t("s.send")}</button></form>
            <select class="input" data-gender="${side}" style="width:auto;font-size:.9rem" aria-label="gender"><option value="">—</option><option value="Female" ${st.gender[side] === "Female" ? "selected" : ""}>Female</option><option value="Male" ${st.gender[side] === "Male" ? "selected" : ""}>Male</option></select>
          </div>`; })}
        </div>
        <div class="stack" id="log" aria-live="polite">${st.turns.length ? st.turns.map(turnCard) : html`<p class="muted center">${t("s.empty")}</p>`}</div>
        ${st.turns.length ? html`<div class="row"><button class="btn sm line" type="button" id="clear">${t("s.clear")}</button></div>` : ""}
      </div>`);
    wire();
  }

  function setNote(side, text) { const n = $(`[data-note="${side}"]`, host); if (n) n.textContent = text; }

  async function play(x) {
    try {
      const lang = x.to; const gender = st.gender[x.side === "a" ? "b" : "a"] || "female";
      const url = await speak(x.turn.translation, lang, gender.toLowerCase());
      player?.pause(); player = new Audio(url); await player.play();
    } catch (e) { toast(msg(e)); }
  }

  async function addTurn(side, request) {
    st.busy = true; setNote(side, t("s.thinking"));
    try {
      const from = side === "a" ? st.a : st.b, to = side === "a" ? st.b : st.a;
      let r;
      if (request.audio) {
        const f = new FormData();
        f.append("audio", request.audio, "clip.wav"); f.append("src", from); f.append("dst", to); f.append("careful", st.careful ? "1" : "0"); f.append("domain", st.domain); if (st.gender[side]) f.append("gender", st.gender[side]);
        r = await api("/api/live/utterance", { method: "POST", form: f });
        if (r.empty) { toast(t("err.mic")); st.busy = false; return draw(); }
        r = r.turn;
      } else {
        r = await api("/api/live/turn", { method: "POST", body: { text: request.text, src: from, dst: to, careful: st.careful, domain: st.domain, gender: st.gender[side] || undefined } });
      }
      const x = { id: crypto.randomUUID(), side, from, to, turn: r, confirmed: false };
      st.turns.push(x); st.busy = false; draw(); sfx("tap");
      if (!r.needs_confirm && r.can_speak) play(x);
    } catch (e) { st.busy = false; draw(); toast(msg(e)); }
  }

  async function mic(side, btn) {
    if (st.rec?.side === side) {
      const r = st.rec; st.rec = null; btn.classList.remove("rec"); setNote(side, t("s.thinking"));
      try { const { blob } = await r.handle.stop(); await addTurn(side, { audio: blob }); } catch (e) { draw(); toast(msg(e)); }
      return;
    }
    if (st.rec) return;
    try {
      const handle = await startRecording({ maxSeconds: 29, onFrame: (bins) => { const l = Math.min(1, bins.slice(2, 20).reduce((a, c) => a + c, 0) / 18 / 160); $$(".bars i", btn).forEach((i, n) => { i.style.height = `${30 + l * 70 * (n % 2 ? 1 : .7)}%`; }); }, onAutoStop: () => mic(side, btn) });
      st.rec = { side, handle }; btn.classList.add("rec"); setNote(side, t("s.hearing"));
    } catch { toast(t("err.mic")); }
  }

  function wire() {
    $("#la", host).addEventListener("change", (e) => { st.a = e.target.value; if (st.a === st.b) st.b = st.a === "en" ? "hi" : "en"; draw(); });
    $("#lb", host).addEventListener("change", (e) => { st.b = e.target.value; if (st.a === st.b) st.a = st.b === "en" ? "hi" : "en"; draw(); });
    $("#swap", host).addEventListener("click", () => { [st.a, st.b] = [st.b, st.a]; draw(); });
    $("#dm", host).addEventListener("change", (e) => { st.domain = e.target.value; st.careful = dom().careful; st.ack = false; draw(); });
    $("#careful", host).addEventListener("change", (e) => { st.careful = e.target.checked; });
    $("#agree", host)?.addEventListener("click", () => { st.ack = true; draw(); });
    $$("[data-say]", host).forEach((b) => b.addEventListener("click", async () => { try { const c = b.dataset.say; const n = (L(c).notices || L("en").notices); const a = new Audio(await speak(`${n.ai_translation} ${n.not_advice}`, c, "female")); await a.play(); } catch (e) { toast(msg(e)); } }));
    $$("[data-mic]", host).forEach((b) => b.addEventListener("click", () => mic(b.dataset.mic, b)));
    $$("[data-type]", host).forEach((f) => f.addEventListener("submit", (e) => { e.preventDefault(); const v = $("input", f).value.trim(); if (v && !st.busy) addTurn(f.dataset.type, { text: v }); }));
    $$("[data-gender]", host).forEach((s) => s.addEventListener("change", () => { st.gender[s.dataset.gender] = s.value; }));
    $$("[data-play]", host).forEach((b) => b.addEventListener("click", () => play(st.turns.find((x) => x.id === b.dataset.play))));
    $$("[data-go]", host).forEach((b) => b.addEventListener("click", () => { const x = st.turns.find((y) => y.id === b.dataset.go); x.confirmed = true; draw(); if (x.turn.can_speak) play(x); }));
    $$("[data-redo]", host).forEach((b) => b.addEventListener("click", () => { st.turns = st.turns.filter((y) => y.id !== b.dataset.redo); draw(); }));
    $("#clear", host)?.addEventListener("click", () => { st.turns = []; draw(); });
  }

  host.classList.add("bench-host");
  mount(host, html`<section class="sheet paper" style="padding-top:2rem"><div class="bench" id="bench"></div></section>`);
  const inner = $("#bench", host); host = inner;
  draw();
  return () => { st.rec?.handle.cancel(); player?.pause(); };
}
