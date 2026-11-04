import { html, mount, $, raw } from "../dom.js";
import { t, getLang } from "../i18n.js";
import { api } from "../api.js";
import { audit } from "./audit.js";
import { highlightChanges } from "../diff.js";
import { icon, mark } from "../icons.js";

const faq = () => [1, 2, 3, 4].map((n) => html`<details><summary>${t(`faq.${n}.q`)}</summary><p>${t(`faq.${n}.a`)}</p></details>`);

const plain = (s) => s.replace(/[*_#`]+/g, "").replace(/\s+/g, " ").trim();

/** A real result from the recorded shaky audit: what was changed, and what the model did about it. */
function specimen(report) {
  const test = report.tests.find((x) => x.status === "broke" && x.output && x.kind === "invariance") || report.tests.find((x) => x.status === "wobble" && x.output);
  if (!test) return "";
  const lang = report.prompt_language;
  return html`<figure class="specimen" style="margin:0">
    <figcaption class="eyebrow">${t("h.spec")}</figcaption>
    <p class="q" lang="${lang}">${report.prompt}</p>
    <div class="pair"><b>${t("h.said")}</b><span lang="${lang}">${plain(report.baseline_output).slice(0, 120)}</span></div>
    <p class="q" lang="${lang}" style="margin-top:1.1rem">${highlightChanges(report.prompt, test.input)}</p>
    <div class="pair"><b>${t("h.said")}</b><span lang="${lang}">${plain(test.output).slice(0, 120)}</span></div>
    <p class="note">${mark("broke", 16)} ${t("h.changed")}</p></figure>`;
}

export function home(host, app) {
  mount(host, html`
    <section class="hero3 wrap">
      <div class="lead">
        <h1 class="h1">${t("h.a")}<br>${t("h.b")}</h1>
        <p class="lede">${t("h.lede")}</p>
        <div class="acts"><a class="btn" href="#bench" id="cta-audit">${t("h.start")}</a><a class="btn line" href="#/translate">${t("nav.translate")}</a><a class="btn line" href="#/setu">${t("h.cta2")}</a></div>
        <p class="hint" style="margin-top:1.2rem">${t("h.note")}</p>
      </div>
      <div id="spec"></div>
    </section>
    <section class="sheet" style="padding-top:1.5rem"><div id="bench" class="bench" style="scroll-margin-top:1.5rem"></div></section>
    <section class="sheet wrap reveal">
      <h2 class="h2" style="max-width:30rem">${t("idx.title")}</h2>
      <ol class="index" style="margin-top:1.6rem">
        ${[["translate", "#/translate"], ["setu", "#/setu"], ["learn", "#/learn"], ["rules", "#/rules"]].map(([id, href], i) => html`<li><b>0${i + 1}</b><h3>${t("nav." + id)}</h3><p>${t(`idx.${id}`)}</p><a class="btn line" href="${href}">${t("idx.open")} ${icon("arrow", 16)}</a></li>`)}
      </ol>
    </section>
    <section class="sheet wrap reveal">
      <h2 class="h2" id="how-title" style="max-width:34rem"></h2>
      <div class="steps">${[1, 2, 3].map((n) => html`<div class="step"><b>0${n}</b><h3>${t(`how.${n}.t`)}</h3><p class="muted">${t(`how.${n}.d`)}</p></div>`)}</div>
    </section>
    <section class="sheet wrap faq reveal" style="max-width:52rem"><h2 class="h2" id="faq-title" style="margin-bottom:1.6rem"></h2>${faq()}</section>
    <footer class="foot wrap"><div class="row"><span>vak · వాక్కు · वाक्</span><span>${t("foot.credit")}</span></div></footer>`);
  // headings that carry <em> come from our own translation tables, so they are set as HTML here
  host.querySelector("#how-title").innerHTML = t("how.title");
  host.querySelector("#faq-title").innerHTML = t("faq.title");
  host.querySelector("#cta-audit").addEventListener("click", (e) => { e.preventDefault(); $("#bench").scrollIntoView({ behavior: "smooth" }); });
  api("/api/demo/shaky").then((d) => { const s = specimen(d.report); if (s) mount($("#spec", host), s); }).catch(() => {});
  return audit($("#bench"), app);
}
