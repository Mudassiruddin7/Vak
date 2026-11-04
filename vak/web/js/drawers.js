// Side drawers: the rupee jars and the history list.
import { $, html, mount, formatInr, timeAgo, el } from "./dom.js";
import { icon } from "./icons.js";
import { t, getLang } from "./i18n.js";
import { api } from "./api.js";
import { state } from "./state.js";

let openCloser = null;

// "53190 days" means nothing to anyone, so long runways are said in months or as "over a year"
function runwayText(days) {
  if (days == null) return t("budget.runwayNone");
  if (days > 365) return t("budget.runwayLong");
  if (days > 60) return t("budget.runwayMonths", { n: Math.round(days / 30) });
  return t("budget.runway", { n: Math.round(days) });
}

function openDrawer(title, build) {
  openCloser?.();
  const scrim = el("div", { class: "scrim", "aria-hidden": "true" });
  const drawer = el("aside", { class: "drawer", role: "dialog", "aria-modal": "true", "aria-label": title });
  const close = () => {
    scrim.remove();
    drawer.remove();
    document.removeEventListener("keydown", onKey);
    openCloser = null;
  };
  const onKey = (e) => { if (e.key === "Escape") close(); };
  document.addEventListener("keydown", onKey);
  scrim.addEventListener("click", close);

  const head = el("div", { class: "drawer-head" }, el("h2", {}, title),
    el("button", { class: "icon-btn", "aria-label": t("nav.close"), onclick: close }));
  head.querySelector("button").innerHTML = icon("x").markup;
  const body = el("div", { class: "drawer-body" });
  drawer.append(head, body);
  document.body.append(scrim, drawer);
  head.querySelector("button").focus();
  build(body, close);
  openCloser = close;
  return close;
}

export async function openBudget() {
  openDrawer(t("budget.title"), async (body) => {
    body.textContent = "…";
    let b;
    try { b = await api.budget(); state.budget = b; } catch { body.textContent = t("err.generic"); return; }
    const pots = [...Object.entries(b.pots).map(([key, p]) => [key, p]), ["buffer", b.buffer]];
    mount(body, html`
      <div class="jars">
        ${pots.map(([key, p]) => {
          const share = p.allocated ? p.remaining / p.allocated : 0;
          return html`<div class="jar" data-low="${share < 0.15 ? 1 : 0}">
            <div class="glass"><div class="fill" style="--fill:0%" data-fill="${Math.round(share * 100)}"></div></div>
            <b>${formatInr(p.remaining)}</b>
            <span>${t(`pot.${key}`)}</span>
            <span>${t("budget.of", { n: formatInr(p.allocated) })}</span>
          </div>`;
        })}
      </div>
      <div class="runway">
        ${t("budget.total")}: <big>${formatInr(b.total.remaining)}</big> ${t("budget.left")}
        <div style="margin-top:6px;font-weight:600;color:var(--ink-2)">
          ${runwayText(b.runway_days)}
        </div>
      </div>`);
    requestAnimationFrame(() => requestAnimationFrame(() => {
      body.querySelectorAll(".fill").forEach((f) => f.style.setProperty("--fill", `${f.dataset.fill}%`));
    }));
  });
}

export async function openHistory(ctx) {
  openDrawer(t("history.title"), async (body, close) => {
    body.textContent = "…";
    let rows;
    try { rows = await api.history(); } catch { body.textContent = t("err.generic"); return; }
    if (!rows.length) { mount(body, html`<p class="empty">${t("history.empty")}</p>`); return; }
    const grade = (s) => (s == null ? "none" : s < 0.6 ? "concerning" : s <= 0.85 ? "mixed" : "steady");
    mount(body, html`
      <div class="hist">
        ${rows.map((r) => html`
          <div style="display:flex;gap:8px;align-items:stretch">
            <button type="button" data-id="${r.id}" style="flex:1">
              <span class="h-score" data-grade="${grade(r.score)}">${r.score == null ? "–" : Math.round(r.score * 100)}</span>
              <span>
                <div class="h-prompt" lang="${/[ఀ-౿]/.test(r.prompt) ? "te" : /[ऀ-ॿ]/.test(r.prompt) ? "hi" : "en"}">${r.prompt}</div>
                <div class="h-meta">${r.target} · ${t(`depth.${r.depth}`)} · ${timeAgo(r.created, getLang())}${r.status === "failed" ? ` · ${t("history.failed")}` : ""}</div>
              </span>
            </button>
            <button type="button" class="icon-btn" data-del="${r.id}" aria-label="${t("history.delete")}" style="align-self:center">${icon("trash")}</button>
          </div>`)}
      </div>`);
    body.addEventListener("click", async (event) => {
      const open = event.target.closest("[data-id]");
      const del = event.target.closest("[data-del]");
      if (open) { close(); ctx.openRecord(open.dataset.id); }
      if (del && window.confirm(`${t("history.delete")}?`)) {
        try { await api.remove(del.dataset.del); del.closest("div[style]").remove(); } catch (err) { ctx.toast(err.message, "bad"); }
      }
    });
  });
}
