// Short lessons built from real, recorded audits: look at what was changed, guess whether the answer moved, then see.
import { html, mount, $, $$ } from "../dom.js";
import { t } from "../i18n.js";
import { api } from "../api.js";
import * as S from "../state.js";
import * as auth from "../auth.js";
import { confetti, sfx } from "../fx.js";

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

export function learn(host, app) {
  const st = { lessons: [], at: 0, q: 0, right: 0, answered: null };
  const lang = () => (["te", "hi"].includes(document.documentElement.lang) ? document.documentElement.lang : "en");

  function path() {
    const s = S.get();
    mount(host, html`<section class="sheet paper" style="padding-top:2rem"><div class="bench">
      <div class="bench-head"><div><h2>${t("l.title")}</h2><p class="muted" style="margin-top:.4rem">${t("l.sub")}</p></div>
        <div class="chips"><span class="chip on">⚡ ${s.xp} ${t("l.xp")}</span><span class="chip">🔥 ${s.streak} ${t("l.streak")}</span></div></div>
      <ol class="path" style="list-style:none;margin:0;padding:1rem 0;display:grid;gap:1.4rem">
        ${st.lessons.map((l, i) => { const done = !!s.done[l.id], open = i === 0 || s.done[st.lessons[i - 1].id], off = [0, 3, 5, 3, 0][i % 5];
          return html`<li style="margin-left:${off}rem;display:flex;align-items:center;gap:1rem">
            <button class="mic" style="width:4.6rem;height:4.6rem;${done ? "background:var(--fathom)" : open ? "" : "background:var(--lumen-dark);color:var(--ink-3);outline-color:var(--line)"}" type="button" data-lesson="${i}" ${open ? "" : "disabled"} aria-label="${L(l.title, lang())}">${done ? "✓" : open ? "★" : "🔒"}</button>
            <div><b style="font:400 1.5rem var(--serif)">${L(l.title, lang())}</b><div class="hint">${open ? `${l.qs.length} · ${t("l.start")}` : t("l.locked")}</div></div></li>`; })}
      </ol></div></section>`);
    $$("[data-lesson]", host).forEach((b) => b.addEventListener("click", () => { st.at = +b.dataset.lesson; st.q = 0; st.right = 0; st.answered = null; sfx("tap"); question(); }));
  }

  function question() {
    const l = st.lessons[st.at], q = l.qs[st.q], sens = ["negation", "control"].includes(q.category);
    const truth = q.verdict !== "same";
    mount(host, html`<section class="sheet paper" style="padding-top:2rem"><div class="bench stack">
      <div class="row"><button class="icon-btn" id="quit" type="button" aria-label="${t("st.close")}">✕</button><div class="meter grow"><i style="width:${(st.q / l.qs.length) * 100}%"></i></div><span class="chip">${st.q + 1}/${l.qs.length}</span></div>
      <h3 class="h3">${sens ? (lang() === "en" ? "The meaning changed. Did the answer?" : t("l.q") === "l.q" ? "Did the answer change?" : t("l.q")) : "Did the answer change?"}</h3>
      <div class="card"><span class="eyebrow">Original</span><p class="say" style="margin-top:.3rem">${q.prompt}</p><p class="muted" style="margin-top:.4rem">→ ${q.base}</p></div>
      <div class="card" style="border-color:var(--ink)"><span class="eyebrow">${q.label}</span><p class="say" style="margin-top:.3rem">${q.input}</p></div>
      <div class="grid2"><button class="btn" type="button" data-ans="1">Yes, it changed</button><button class="btn line" type="button" data-ans="0">No, same answer</button></div>
      <div id="sheet"></div></div></section>`);
    $("#quit", host).addEventListener("click", path);
    $$("[data-ans]", host).forEach((b) => b.addEventListener("click", () => {
      if (st.answered !== null) return;
      const guess = b.dataset.ans === "1"; st.answered = guess === truth; if (st.answered) { st.right++; S.earn(10); } sfx(st.answered ? "ok" : "wrong");
      const why = sens ? (truth ? "Good: the meaning changed, so the answer should change too." : "A flaw: the meaning changed but the answer stayed the same.")
        : truth ? `It changed, though only the ${q.category === "typo" ? "spelling" : "form"} was different. That is a soft spot.` : "Same meaning, same answer. That is what a steady model does.";
      $("#sheet", host).innerHTML = "";
      const box = document.createElement("div"); box.className = "card stack"; box.style.cssText = `border:2px solid ${st.answered ? "var(--fathom)" : "var(--pulse)"};background:${st.answered ? "var(--leaf)" : "color-mix(in srgb, var(--pulse) 12%, var(--card))"};color:var(--vast)`;
      box.setAttribute("role", "status");
      const h = document.createElement("b"); h.textContent = st.answered ? t("l.right") : t("l.wrong"); const p = document.createElement("p"); p.textContent = why + (q.reason ? ` (${q.reason})` : "");
      const next = document.createElement("button"); next.className = "btn dark"; next.type = "button"; next.textContent = `${t("l.next")} →`;
      box.append(h, p, next); $("#sheet", host).append(box); next.focus();
      next.addEventListener("click", () => { st.answered = null; st.q++; if (st.q >= l.qs.length) finish(); else question(); });
    }));
  }

  function finish() {
    const l = st.lessons[st.at]; const done = { ...S.get().done, [l.id]: true }; S.set({ done }); S.earn(20); confetti(); sfx("done");
    if (auth.user()) auth.saveProgress({ xp: S.get().xp, streak: S.get().streak, last_day: S.get().lastDay, done }).catch(() => {});
    mount(host, html`<section class="sheet paper" style="padding-top:2rem"><div class="bench stack center"><h2 class="h2">${t("l.done")}</h2><p class="lede">${st.right}/${l.qs.length} · +${st.right * 10 + 20} ${t("l.xp")}</p><div class="row" style="justify-content:center"><button class="btn" id="back" type="button">${t("l.again")}</button></div></div></section>`);
    $("#back", host).addEventListener("click", path);
  }

  mount(host, html`<section class="sheet paper" style="padding-top:2rem"><div class="bench"><p class="muted">…</p></div></section>`);
  Promise.all(["steady", "shaky"].map((n) => api(`/api/demo/${n}`).then((d) => d.report))).then((rs) => { st.lessons = build(rs); path(); }).catch(() => path());
  return () => {};
}
