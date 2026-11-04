// Short lessons built from real, recorded audits: look at what was changed, guess whether the answer moved, then see.
import { html, mount, $, $$ } from "../dom.js";
import { t } from "../i18n.js";
import { api } from "../api.js";
import * as S from "../state.js";
import * as auth from "../auth.js";
import { sfx } from "../fx.js";
import { icon, mark } from "../icons.js";

const TOPICS = [
  { id: "noise", cats: ["typo", "format", "numerals"], title: { en: "Typing slips", hi: "टाइपिंग की चूक", te: "టైపింగ్ తప్పులు" } },
  { id: "wording", cats: ["paraphrase", "polite"], title: { en: "Other words, same ask", hi: "दूसरे शब्द, वही सवाल", te: "వేరే మాటలు, అదే ప్రశ్న" } },
  { id: "script", cats: ["script", "language"], title: { en: "Other scripts and languages", hi: "दूसरी लिपि और भाषा", te: "వేరే లిపి, భాష" } },
  { id: "noise2", cats: ["distractor"], title: { en: "Side remarks", hi: "बीच की बातें", te: "పక్క మాటలు" } },
  { id: "meaning", cats: ["negation", "control"], title: { en: "When the meaning really changes", hi: "जब मतलब सच में बदलता है", te: "అర్థం నిజంగా మారినప్పుడు" } },
];
const L = (obj, lang) => obj[lang] || obj.en;

function build(reports) {
  const pool = reports.flatMap((r) => r.tests.filter((x) => x.output && x.verdict).map((x) => ({ ...x, prompt: r.prompt, base: r.baseline_output })));
  return TOPICS.map((tp) => {
    const items = pool.filter((x) => tp.cats.includes(x.category));
    // alternate the answers so the right choice is not always the same one
    const changed = items.filter((x) => x.verdict !== "same"), same = items.filter((x) => x.verdict === "same");
    const mix = []; for (let i = 0; mix.length < 5 && (changed[i] || same[i]); i++) { if (same[i]) mix.push(same[i]); if (changed[i] && mix.length < 5) mix.push(changed[i]); }
    return { ...tp, qs: mix };
  }).filter((l) => l.qs.length >= 3);
}

export function learn(host) {
  const st = { lessons: [], at: 0, q: 0, right: 0, answered: null };
  const lang = () => (["te", "hi"].includes(document.documentElement.lang) ? document.documentElement.lang : "en");
  const shell = (inner) => mount(host, html`<section class="sheet" style="padding-top:1.6rem"><div class="bench">${inner}</div></section>`);

  function path() {
    const s = S.get();
    shell(html`
      <div class="bench-head"><div><h2>${t("l.title")}</h2><p class="muted" style="margin-top:.4rem;max-width:34rem">${t("l.sub")}</p></div>
        <p class="muted"><b style="font-weight:560;color:var(--ink)">${s.xp}</b> ${t("l.xp")} · <b style="font-weight:560;color:var(--ink)">${s.streak}</b> ${t("l.streak")}</p></div>
      <ol class="index">${st.lessons.map((l, i) => { const done = !!s.done[l.id], open = i === 0 || s.done[st.lessons[i - 1].id];
        return html`<li style="grid-template-columns:3rem 1fr auto"><b>0${i + 1}</b><div><h3 style="font-size:1.35rem">${L(l.title, lang())}</h3><p class="hint">${open ? `${l.qs.length} · ${t("l.start")}` : t("l.locked")}</p></div>
          ${done ? html`<span class="mk ok">${mark("ok", 22)}</span>` : open ? html`<button class="btn sm" type="button" data-lesson="${i}">${t("l.start")}</button>` : html`<span class="hint">${icon("lock", 18)}</span>`}</li>`; })}</ol>`);
    $$("[data-lesson]", host).forEach((b) => b.addEventListener("click", () => { st.at = +b.dataset.lesson; st.q = 0; st.right = 0; st.answered = null; sfx("tap"); question(); }));
  }

  function question() {
    const l = st.lessons[st.at], q = l.qs[st.q], sens = ["negation", "control"].includes(q.category);
    const truth = q.verdict !== "same";
    shell(html`
      <div class="row"><button class="icon-btn" id="quit" type="button" aria-label="${t("st.close")}">${icon("close", 18)}</button><div class="meter grow"><i style="width:${(st.q / l.qs.length) * 100}%"></i></div><span class="hint">${st.q + 1}/${l.qs.length}</span></div>
      <div class="stack" style="margin-top:1.6rem;max-width:42rem">
        <h3 class="h3">${sens ? t("l.q.meaning") : t("l.q")}</h3>
        <div class="card"><p class="hint">${t("l.orig")}</p><p class="say" dir="auto" style="margin-top:.2rem">${q.prompt}</p><p class="muted" dir="auto" style="margin-top:.4rem">→ ${q.base}</p></div>
        <div class="card"><p class="hint">${q.label}</p><p class="say" dir="auto" style="margin-top:.2rem">${q.input}</p></div>
        <div class="row"><button class="btn" type="button" data-ans="1">${t("l.yes")}</button><button class="btn line" type="button" data-ans="0">${t("l.no")}</button></div>
        <div id="sheet"></div></div>`);
    $("#quit", host).addEventListener("click", path);
    $$("[data-ans]", host).forEach((b) => b.addEventListener("click", () => {
      if (st.answered !== null) return;
      const guess = b.dataset.ans === "1"; st.answered = guess === truth; if (st.answered) { st.right++; S.earn(10); } sfx(st.answered ? "ok" : "wrong");
      const why = sens ? (truth ? t("l.why.sens.yes") : t("l.why.sens.no")) : truth ? t("l.why.yes") : t("l.why.no");
      const box = document.createElement("div"); box.className = "err-box stack"; box.style.cssText = `gap:.5rem;border-inline-start-color:${st.answered ? "var(--held)" : "var(--accent)"};background:${st.answered ? "var(--sage)" : "var(--clay)"}`;
      box.setAttribute("role", "status");
      const h = document.createElement("b"); h.style.fontWeight = "560"; h.textContent = st.answered ? t("l.right") : t("l.wrong");
      const p = document.createElement("p"); p.textContent = why + (q.reason ? ` (${q.reason})` : "");
      const next = document.createElement("button"); next.className = "btn sm"; next.type = "button"; next.textContent = t("l.next");
      box.append(h, p, next); $("#sheet", host).replaceChildren(box); next.focus();
      next.addEventListener("click", () => { st.answered = null; st.q++; if (st.q >= l.qs.length) finish(); else question(); });
    }));
  }

  function finish() {
    const l = st.lessons[st.at]; const done = { ...S.get().done, [l.id]: true }; S.set({ done }); S.earn(20); sfx("done");
    if (auth.user()) auth.saveProgress({ xp: S.get().xp, streak: S.get().streak, last_day: S.get().lastDay, done }).catch(() => {});
    shell(html`<div class="stack" style="max-width:34rem"><h2 class="h2">${t("l.done")}</h2><p class="lede">${st.right}/${l.qs.length} · +${st.right * 10 + 20} ${t("l.xp")}</p><div class="row"><button class="btn" id="back" type="button">${t("l.again")}</button></div></div>`);
    $("#back", host).addEventListener("click", path);
  }

  shell(html`<p class="muted">…</p>`);
  Promise.all(["steady", "shaky"].map((n) => api(`/api/demo/${n}`).then((d) => d.report))).then((rs) => { st.lessons = build(rs); path(); }).catch(() => path());
  return () => {};
}
