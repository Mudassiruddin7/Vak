import { html, mount, $, raw } from "../dom.js";
import { t, getLang } from "../i18n.js";
import { audit } from "./audit.js";

const PHRASES = ["ఒక కిలో బియ్యం ధర ఎంత?", "दिल्ली से हैदराबाद कितने घंटे?", "இந்தப் படம் எப்படி இருக்கு?", "ಈ ಬಸ್ ಯಾವಾಗ ಬರುತ್ತದೆ?", "What is the price of one kilo of rice?", "এই ওষুধ কখন খাব?"];

function ribbon() {
  const line = PHRASES.join("   •   ") + "   •   ";
  return html`
  <div class="ribbon" aria-hidden="true">
    <svg class="ring" viewBox="0 0 200 200"><defs><path id="circ" d="M100,100 m-82,0 a82,82 0 1,1 164,0 a82,82 0 1,1 -164,0"/></defs>
      <text><textPath href="#circ">${t("ribbon.a")} • ${t("ribbon.a")} •</textPath></text></svg>
    <svg viewBox="0 0 1200 320" preserveAspectRatio="xMidYMid slice">
      <defs><path id="band" d="M 560 292 C 760 292, 930 246, 1260 96"/><path id="ghost" d="M -60 232 C 160 214, 380 300, 560 292"/></defs>
      <g class="ghost"><text><textPath href="#ghost">${line}</textPath></text></g>
      <use href="#band" fill="none" stroke="var(--vast)" stroke-width="52" stroke-linecap="round"/>
      <text><textPath href="#band" startOffset="0%">${line}${line}<animate attributeName="startOffset" from="0%" to="-48%" dur="48s" repeatCount="indefinite"/></textPath></text>
    </svg>
    <a class="voicebar" href="#bench" aria-label="${t("a.mic")}">${raw(Array.from({ length: 16 }, () => "<i></i>").join(""))}</a>
  </div>`;
}

const faq = () => [1, 2, 3, 4].map((n) => html`<details><summary>${t(`faq.${n}.q`)}</summary><p>${t(`faq.${n}.a`)}</p></details>`);

export function home(host, app) {
  mount(host, html`
    <section class="hero wrap">
      <p class="eyebrow">${t("h.eyebrow")}</p>
      <h1 class="h1">${t("h.a")}<br><em>${t("h.b")}</em></h1>
      <p class="lede">${t("h.lede")}</p>
      <div class="cta-row"><a class="btn" href="#bench" id="cta-audit">${t("h.cta1")}</a><a class="btn line" href="#/setu">${t("h.cta2")}</a></div>
      <small>${t("h.note")}</small>
      ${ribbon()}
    </section>
    <section class="sheet paper" id="bench-sheet"><div id="bench" class="bench" style="scroll-margin-top:6rem"></div></section>
    <section class="sheet ink reveal"><div class="wrap">
      <p class="eyebrow">${t("how.eyebrow")}</p><h2 class="h2" id="how-title" style="margin-top:.8rem"></h2>
      <div class="steps">${[1, 2, 3].map((n) => html`<div class="step"><b>${n}</b><h3>${t(`how.${n}.t`)}</h3><p>${t(`how.${n}.d`)}</p></div>`)}</div>
    </div></section>
    <section class="sheet paper reveal"><div class="wrap faq" style="max-width:52rem"><h2 class="h2" id="faq-title" style="margin-bottom:2rem"></h2>${faq()}</div></section>
    <footer class="sheet teal foot"><div class="wrap">
      <div class="row"><span>Vak · వాక్కు · वाक्</span><span>Built on Sarvam’s speech, translation and language APIs.</span></div>
      <div class="big" aria-hidden="true">Vak</div></div></footer>`);
  // headings that carry <em> markup come from trusted translation tables, so they are set as HTML here
  host.querySelector("#how-title").innerHTML = t("how.title");
  host.querySelector("#faq-title").innerHTML = t("faq.title");
  host.querySelector("#cta-audit").addEventListener("click", (e) => { e.preventDefault(); $("#bench").scrollIntoView({ behavior: "smooth" }); });
  return audit($("#bench"), app);
}
