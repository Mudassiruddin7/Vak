// Pieces used by more than one view.
import { html, raw } from "../dom.js";
import { icon, CATEGORY_ICON } from "../icons.js";
import { highlightChanges } from "../diff.js";
import { getLang, hasKey, t } from "../i18n.js";

export function categoryLabel(test) {
  if (getLang() === "en") return test.label || test.category;
  return hasKey(`cat.${test.category}`) ? t(`cat.${test.category}`) : test.label;
}

// One card per variation of the question. `test` may be a bare variant (still waiting) or a finished result.
export function tryCard(test, index, original, promptLang, status) {
  const state = status || test.status || "waiting";
  const isResample = test.category === "resample";
  const shown = isResample ? html`${test.input}` : highlightChanges(original, test.input);
  const output = test.output;
  return html`
    <article class="try expandable" data-id="${test.id}" data-state="${state}" style="--n:${index}" tabindex="0" role="button" aria-expanded="false">
      <div class="t-top">
        <span class="t-ico">${icon(CATEGORY_ICON[test.category] || "search")}</span>
        <span class="t-label">${categoryLabel(test)}</span>
        <span class="t-state">${t(`state.${state}`)}</span>
      </div>
      <p class="t-input" lang="${promptLang}">${shown}</p>
      <div class="t-detail">
        <div><span class="k">${t("detail.input")}</span><p lang="${promptLang}">${test.input}</p></div>
        <div class="t-out"><span class="k">${t("detail.output")}</span><p>${output ?? test.reason ?? ""}</p></div>
        ${test.reason && output ? html`<div><span class="k">${t("detail.why")}</span><p class="why">${test.reason}</p></div>` : ""}
      </div>
    </article>`;
}

// Cards expand and collapse on click or Enter/Space
export function wireCards(root) {
  const toggle = (card) => {
    const open = card.classList.toggle("is-open");
    card.setAttribute("aria-expanded", String(open));
  };
  root.addEventListener("click", (e) => {
    const card = e.target.closest(".try");
    if (card && root.contains(card)) toggle(card);
  });
  root.addEventListener("keydown", (e) => {
    const card = e.target.closest(".try");
    if (card && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); toggle(card); }
  });
}

export function setCardState(root, id, test) {
  const card = root.querySelector(`.try[data-id="${CSS.escape(id)}"]`);
  if (!card) return null;
  const state = test.status || test.state || "waiting";
  card.dataset.state = state;
  card.querySelector(".t-state").textContent = t(`state.${state}`);
  const out = card.querySelector(".t-out p");
  if (out && (test.output ?? test.reason)) out.textContent = test.output ?? test.reason;
  const why = test.reason && test.output ? test.reason : "";
  let whyBox = card.querySelector(".why");
  if (why && !whyBox) {
    const wrap = document.createElement("div");
    wrap.innerHTML = `<span class="k"></span><p class="why"></p>`;
    wrap.querySelector(".k").textContent = t("detail.why");
    wrap.querySelector(".why").textContent = why;
    card.querySelector(".t-detail").append(wrap);
  } else if (whyBox) {
    whyBox.textContent = why;
  }
  return card;
}

export function spinner() { return raw('<span class="spinner" aria-hidden="true"></span>'); }
