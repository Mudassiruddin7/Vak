// The hero's colour field: three soft blobs drifting slowly, and one big word that fades from script to
// script. Nothing here moves fast; it is meant to be looked at for a moment and then ignored.
const WORDS = [["వాక్కు", "Telugu"], ["वाक्", "Hindi"], ["பேச்சு", "Tamil"], ["বাক্", "Bengali"], ["ಮಾತು", "Kannada"], ["Vak", "English"], ["വാക്ക്", "Malayalam"], ["વાણી", "Gujarati"]];
const FONT = '"Noto Sans Telugu","Noto Sans Devanagari","Noto Sans Tamil","Noto Sans Bengali","Noto Sans Kannada","Noto Sans Malayalam","Noto Sans Gujarati","Instrument Sans",sans-serif';

export async function pulli(box, tag) {
  const cv = document.createElement("canvas"); box.prepend(cv);
  const ctx = cv.getContext("2d");
  const still = document.documentElement.hasAttribute("data-still") || matchMedia("(prefers-reduced-motion: reduce)").matches;
  const css = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  let W = 0, H = 0, dpr = 1, idx = 0, started = performance.now(), raf = 0, dead = false;
  await Promise.all(WORDS.map(([w]) => document.fonts.load(`500 80px ${FONT}`, w).catch(() => {})));
  const size = () => { dpr = Math.min(2, devicePixelRatio || 1); W = box.clientWidth; H = box.clientHeight; cv.width = W * dpr; cv.height = H * dpr; };
  const blob = (x, y, r, color, a) => { const g = ctx.createRadialGradient(x, y, 0, x, y, r); g.addColorStop(0, color); g.addColorStop(1, "transparent"); ctx.globalAlpha = a; ctx.fillStyle = g; ctx.fillRect(0, 0, W, H); };

  function frame(now) {
    if (dead) return;
    const t = (now - started) / 1000;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.clearRect(0, 0, W, H);
    blob(W * (0.3 + 0.08 * Math.sin(t * 0.21)), H * (0.35 + 0.1 * Math.cos(t * 0.17)), W * 0.55, css("--leaf"), 1);
    blob(W * (0.72 + 0.07 * Math.cos(t * 0.19)), H * (0.4 + 0.1 * Math.sin(t * 0.23)), W * 0.5, css("--sky"), 1);
    blob(W * (0.5 + 0.09 * Math.sin(t * 0.15 + 2)), H * (0.8 + 0.06 * Math.cos(t * 0.2)), W * 0.5, css("--peach"), 1);
    // one word at a time: 7 s each, fading in and out
    const slot = 7, k = (t % slot) / slot, word = WORDS[Math.floor(t / slot) % WORDS.length], fade = Math.min(1, k / 0.18, (1 - k) / 0.18);
    if (tag && tag.textContent !== word[1]) tag.textContent = word[1];
    ctx.globalAlpha = Math.max(0, fade) * 0.92; ctx.fillStyle = css("--ink"); ctx.textAlign = "center"; ctx.textBaseline = "middle";
    let px = H * 0.34; ctx.font = `500 ${px}px ${FONT}`; const w = ctx.measureText(word[0]).width; if (w > W * 0.76) { px *= (W * 0.76) / w; ctx.font = `500 ${px}px ${FONT}`; }
    ctx.fillText(word[0], W / 2, H * 0.48 + (1 - fade) * 6);
    ctx.globalAlpha = 1;
    if (!still) raf = requestAnimationFrame(frame);
  }
  size(); if (tag) tag.textContent = WORDS[0][1];
  frame(performance.now());
  if (still) return () => {};
  raf = requestAnimationFrame(frame);
  const ro = new ResizeObserver(size); ro.observe(box);
  return () => { dead = true; cancelAnimationFrame(raf); ro.disconnect(); };
}
