// The hero's dot field: a kolam-style grid of dots, a thread looping around them, and dots that light
// up into a word in a different Indian script every few seconds.
const WORDS = [["వాక్కు", "Telugu"], ["वाक्", "Hindi"], ["பேச்சு", "Tamil"], ["বাক্", "Bengali"], ["ಮಾತು", "Kannada"], ["Vak", "English"], ["വാക്ക്", "Malayalam"], ["વાણી", "Gujarati"]];
const FONT = '"Noto Sans Telugu","Noto Sans Devanagari","Noto Sans Tamil","Noto Sans Bengali","Noto Sans Kannada","Noto Sans Malayalam","Noto Sans Gujarati","Instrument Sans",sans-serif';

export async function pulli(box, tag) {
  const cv = document.createElement("canvas"); box.prepend(cv);
  const ctx = cv.getContext("2d");
  const still = document.documentElement.hasAttribute("data-still") || matchMedia("(prefers-reduced-motion: reduce)").matches;
  const css = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  let W = 0, H = 0, dpr = 1, pitch = 26, cols = 0, rows = 0, lit = [], target = [], idx = -1, raf = 0, dead = false;
  const trail = [];

  async function sample(i) {
    const [word] = WORDS[i];
    await Promise.all(WORDS.map(([w]) => document.fonts.load(`700 80px ${FONT}`, w).catch(() => {})));
    const off = document.createElement("canvas"); off.width = cols; off.height = rows;
    const o = off.getContext("2d", { willReadFrequently: true });
    let size = rows * 0.62; o.font = `700 ${size}px ${FONT}`;
    const fit = (cols * 0.86) / Math.max(1, o.measureText(word).width); if (fit < 1) { size *= fit; o.font = `700 ${size}px ${FONT}`; }
    o.textAlign = "center"; o.textBaseline = "middle"; o.fillStyle = "#000"; o.fillText(word, cols / 2, rows / 2 + size * 0.04);
    const d = o.getImageData(0, 0, cols, rows).data, out = new Float32Array(cols * rows);
    for (let k = 0; k < out.length; k++) out[k] = d[k * 4 + 3] > 110 ? 1 : 0;
    return out;
  }

  function resize() {
    dpr = Math.min(2, devicePixelRatio || 1); W = box.clientWidth; H = box.clientHeight;
    cv.width = W * dpr; cv.height = H * dpr;
    pitch = Math.max(16, Math.round(W / 30)); cols = Math.floor(W / pitch); rows = Math.floor(H / pitch);
    lit = new Float32Array(cols * rows); target = new Float32Array(cols * rows);
  }

  async function next() {
    idx = (idx + 1) % WORDS.length; target = await sample(idx); if (tag) tag.textContent = WORDS[idx][1];
  }

  function frame(t) {
    if (dead) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.clearRect(0, 0, W, H);
    const ink = css("--ink"), glow = css("--glow"), thread = css("--fathom");
    const ox = (W - cols * pitch) / 2 + pitch / 2, oy = (H - rows * pitch) / 2 + pitch / 2;
    for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) {
      const k = r * cols + c; lit[k] += (target[k] - lit[k]) * 0.09;
      const x = ox + c * pitch, y = oy + r * pitch, a = lit[k];
      ctx.globalAlpha = 0.22 + a * 0.78; ctx.fillStyle = a > 0.5 ? glow : ink;
      ctx.beginPath(); ctx.arc(x, y, 1.5 + a * (pitch * 0.2), 0, 7); ctx.fill();
    }
    // the thread: one point looping around the grid, leaving a fading trail
    const s = t / 1000, px = W / 2 + Math.sin(s * 0.9) * W * 0.38 + Math.sin(s * 2.3) * W * 0.05, py = H / 2 + Math.sin(s * 1.3 + 1) * H * 0.36 + Math.cos(s * 2.1) * H * 0.04;
    trail.push([px, py]); if (trail.length > 70) trail.shift();
    ctx.globalAlpha = 1; ctx.lineCap = "round"; ctx.lineJoin = "round"; ctx.strokeStyle = thread;
    for (let i = 1; i < trail.length; i++) { ctx.globalAlpha = i / trail.length * 0.9; ctx.lineWidth = 1 + (i / trail.length) * 3.5; ctx.beginPath(); ctx.moveTo(...trail[i - 1]); ctx.lineTo(...trail[i]); ctx.stroke(); }
    ctx.globalAlpha = 1;
    raf = requestAnimationFrame(frame);
  }

  resize(); await next();
  if (still) { lit.set(target); frame(0); cancelAnimationFrame(raf); return () => {}; }
  raf = requestAnimationFrame(frame);
  const timer = setInterval(next, 3400);
  const ro = new ResizeObserver(() => { resize(); next(); }); ro.observe(box);
  return () => { dead = true; cancelAnimationFrame(raf); clearInterval(timer); ro.disconnect(); };
}
