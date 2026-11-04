import { html, mount, $, $$ } from "../dom.js";
import { t, getLang } from "../i18n.js";
import { api } from "../api.js";

export function rules(host, app) {
  const langs = app.config.languages;
  const st = { code: ["te", "hi"].includes(getLang()) ? getLang() : "te", reg: null, res: null };
  const L = () => langs.find((l) => l.code === st.code);

  function draw() {
    const l = L();
    mount(host, html`<section class="sheet paper" style="padding-top:2rem"><div class="bench stack">
      <div class="bench-head"><div><h2>${t("r.title")}</h2><p class="muted" style="margin-top:.4rem">${t("r.sub")}</p></div>
        <label class="field" style="min-width:14rem">${t("r.pick")}<select class="input" id="pick">${langs.filter((x) => x.tier === "full").map((x) => html`<option value="${x.code}" ${x.code === st.code ? "selected" : ""}>${x.native} · ${x.english}</option>`)}</select></label></div>
      <div class="grid2">
        <div class="card stack"><span class="eyebrow">${t("r.max")}</span><div style="font:400 4rem/1 var(--serif)">${l.max_words}<small style="font:500 1rem var(--sans);color:var(--ink-3)"> ${t("r.words")}</small></div>
          <p class="hint">${l.unreviewed ? t("a.unrev") : "Hand-written wording."}</p></div>
        <div class="card stack"><span class="eyebrow">${t("r.you")}</span><p>${l.you}</p><span class="eyebrow">${t("r.reg")}</span><p>${l.register}</p></div>
        <div class="card stack"><span class="eyebrow">${t("r.digits")}</span><p>${l.digits}</p><span class="eyebrow">${t("r.loan")}</span><p>${l.loanwords || "—"}</p></div>
        <div class="card stack"><span class="eyebrow">${t("r.script")}</span>${l.script_notes.length ? html`<ul style="margin:0;padding-left:1.1rem">${l.script_notes.map((n) => html`<li>${n}</li>`)}</ul>` : html`<p>—</p>`}</div>
      </div>
      <div class="card stack"><h3 class="h3" style="font-size:1.5rem">${t("r.check")}</h3>
        <textarea class="input" id="txt" lang="${l.code}" placeholder="${t("r.ph")}" style="min-height:6rem"></textarea>
        <div class="row"><button class="btn sm" id="chk" type="button">${t("r.run")}</button></div><div id="out" role="status"></div></div>
      <div class="stack"><h3 class="h3" style="font-size:1.5rem">${t("r.regs")}</h3><p class="hint">${t("r.verify")}</p>
        <div class="grid2">${(st.reg?.items || []).map((i) => html`<article class="card stack"><b>${i.title}</b><p class="muted">${i.summary}</p>${i.source ? html`<a href="${i.source}" rel="noopener noreferrer" target="_blank" class="hint">${i.source.replace(/^https?:\/\//, "")}</a>` : ""}</article>`)}</div></div>
    </div></section>`);
    $("#pick", host).addEventListener("change", (e) => { st.code = e.target.value; draw(); });
    $("#chk", host).addEventListener("click", async () => {
      const out = $("#out", host); out.textContent = "…";
      try {
        const r = await api("/api/check", { method: "POST", body: { text: $("#txt", host).value, lang: st.code } });
        const bad = [];
        if (r.long_sentences.length) bad.push(html`<p><b>${t("r.long")}:</b></p><ul>${r.long_sentences.map((s) => html`<li lang="${st.code}">${s}</li>`)}</ul>`);
        if (r.banned.length) bad.push(html`<p><b>${t("r.banned")}:</b> ${r.banned.join(", ")}</p>`);
        if (r.wrong_script) bad.push(html`<p><b>${t("r.wrong")}</b></p>`);
        mount(out, bad.length ? html`<div class="err-box">${bad}</div>` : html`<div class="chip ok">${t("r.ok")}</div>`);
      } catch (e) { out.textContent = e.message; }
    });
  }
  fetch("/data/regulations.json").then((r) => r.json()).then((j) => { st.reg = j; draw(); }).catch(draw);
  draw();
  return () => {};
}
