// node scripts/demo-art.mjs <data dir> [DevTools port, default 9222]: draws a cover, background and
// icon for every game in <data dir>/demo-art.json (written by the demo_library example) in the
// running hidden app's page (canvas) and saves them into the data dir. See docs/screenshots.md.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
const [dir, port = "9222"] = process.argv.slice(2);
const games = JSON.parse(readFileSync(join(dir, "demo-art.json"), "utf8"));
const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = list.find((p) => p.type === "page" && p.url.startsWith("http"));
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r));
let id = 0;
const pending = new Map();
ws.addEventListener("message", (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
});
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });

const draw = String.raw`
(game) => {
  const seed = [...game.name].reduce((a, c) => (a * 31 + c.charCodeAt(0)) >>> 0, 7);
  let s = seed;
  const rnd = () => ((s = (s * 1103515245 + 12345) >>> 0) / 4294967296);
  const h = game.hue;
  const hsl = (dh, sat, l, a = 1) => 'hsla(' + ((h + dh + 360) % 360) + ',' + sat + '%,' + l + '%,' + a + ')';
  const motif = ['mountains', 'waves', 'city', 'stars', 'forest', 'rings'][seed % 6];
  function scene(ctx, W, H, title) {
    const sky = ctx.createLinearGradient(0, 0, 0, H);
    sky.addColorStop(0, hsl(-20, 55, 14));
    sky.addColorStop(0.55, hsl(10, 60, 34));
    sky.addColorStop(1, hsl(30, 70, 58));
    ctx.fillStyle = sky; ctx.fillRect(0, 0, W, H);
    // Stars
    for (let i = 0; i < 120; i++) {
      ctx.fillStyle = 'rgba(255,255,255,' + (0.2 + rnd() * 0.6) + ')';
      const r = rnd() * 1.6 * (W / 600);
      ctx.beginPath(); ctx.arc(rnd() * W, rnd() * H * 0.55, r, 0, 7); ctx.fill();
    }
    // Sun or moon with glow
    const cx = W * (0.25 + rnd() * 0.5), cy = H * (title ? 0.5 + rnd() * 0.08 : 0.3 + rnd() * 0.15), R = Math.min(W, H) * (0.1 + rnd() * 0.08);
    const glow = ctx.createRadialGradient(cx, cy, R * 0.2, cx, cy, R * 4);
    glow.addColorStop(0, hsl(40, 90, 80, 0.55)); glow.addColorStop(1, hsl(40, 90, 60, 0));
    ctx.fillStyle = glow; ctx.fillRect(0, 0, W, H);
    ctx.fillStyle = hsl(45, 95, 85); ctx.beginPath(); ctx.arc(cx, cy, R, 0, 7); ctx.fill();
    if (motif === 'rings') {
      ctx.strokeStyle = hsl(50, 80, 80, 0.7); ctx.lineWidth = R * 0.12;
      ctx.beginPath(); ctx.ellipse(cx, cy, R * 2, R * 0.5, -0.3, 0, 7); ctx.stroke();
    }
    // Layers
    for (let layer = 0; layer < 4; layer++) {
      const base = H * (0.55 + layer * 0.11);
      ctx.fillStyle = hsl(-30 + layer * 8, 45, 22 - layer * 4);
      ctx.beginPath(); ctx.moveTo(0, H);
      if (motif === 'city') {
        let x = 0;
        while (x < W) { const bw = W * (0.04 + rnd() * 0.07); const bh = H * (0.05 + rnd() * 0.25) * (1 - layer * 0.2); ctx.lineTo(x, base - bh); ctx.lineTo(x + bw, base - bh); x += bw; }
        ctx.lineTo(W, base);
      } else if (motif === 'waves') {
        for (let x = 0; x <= W; x += W / 60) ctx.lineTo(x, base - Math.sin(x / W * 12 + layer * 2 + seed) * H * 0.02 - layer * 4);
      } else if (motif === 'forest') {
        for (let x = 0; x <= W; x += W / 40) { const t = H * (0.06 + rnd() * 0.12); ctx.lineTo(x, base); ctx.lineTo(x + W / 80, base - t); }
        ctx.lineTo(W, base);
      } else {
        let y = base - H * 0.08;
        for (let x = 0; x <= W; x += W / 12) { y = base - H * (0.04 + rnd() * 0.22) * (1 - layer * 0.15); ctx.lineTo(x, y); }
      }
      ctx.lineTo(W, H); ctx.closePath(); ctx.fill();
    }
    // Vignette
    const v = ctx.createRadialGradient(W / 2, H / 2, Math.min(W, H) * 0.3, W / 2, H / 2, Math.max(W, H) * 0.75);
    v.addColorStop(0, 'rgba(0,0,0,0)'); v.addColorStop(1, 'rgba(0,0,0,0.45)');
    ctx.fillStyle = v; ctx.fillRect(0, 0, W, H);
    if (title) {
      const words = game.name.split(' ');
      const lines = [];
      let line = '';
      const size = Math.round(W * 0.1);
      ctx.font = '800 ' + size + 'px "Segoe UI", sans-serif';
      for (const word of words) { const t = line ? line + ' ' + word : word; if (ctx.measureText(t.toUpperCase()).width > W * 0.8 && line) { lines.push(line); line = word; } else line = t; }
      lines.push(line);
      ctx.textAlign = 'center'; ctx.textBaseline = 'alphabetic';
      ctx.shadowColor = 'rgba(0,0,0,0.6)'; ctx.shadowBlur = W * 0.03;
      ctx.fillStyle = '#fff';
      lines.forEach((l, i) => ctx.fillText(l.toUpperCase(), W / 2, H * 0.1 + size + i * size * 1.08, W * 0.9));
      ctx.shadowBlur = 0;
      ctx.font = '600 ' + Math.round(W * 0.035) + 'px "Segoe UI", sans-serif';
      ctx.fillStyle = 'rgba(255,255,255,0.8)';
      ctx.fillText(game.developer.toUpperCase(), W / 2, H * 0.95);
    }
  }
  const make = (W, H, title, type, q) => { const c = document.createElement('canvas'); c.width = W; c.height = H; scene(c.getContext('2d'), W, H, title); return c.toDataURL(type, q); };
  s = seed; const cover = make(600, 900, true, 'image/png');
  s = seed; const background = make(1920, 1080, false, 'image/jpeg', 0.88);
  const ic = document.createElement('canvas'); ic.width = 32; ic.height = 32;
  const x = ic.getContext('2d');
  const g = x.createLinearGradient(0, 0, 32, 32); g.addColorStop(0, hsl(-20, 60, 30)); g.addColorStop(1, hsl(30, 70, 55));
  x.fillStyle = g; x.beginPath(); x.roundRect(0, 0, 32, 32, 7); x.fill();
  x.fillStyle = '#fff'; x.font = '800 19px "Segoe UI", sans-serif'; x.textAlign = 'center'; x.textBaseline = 'middle';
  x.fillText(game.name[0], 16, 17);
  return { cover, background, icon: ic.toDataURL('image/png') };
}`;

for (const game of games) {
  const res = await send("Runtime.evaluate", { expression: `(${draw})(${JSON.stringify(game)})`, returnByValue: true });
  const v = res.result?.result?.value;
  if (!v) { console.error(game.name, JSON.stringify(res.result).slice(0, 400)); process.exit(1); }
  for (const kind of ["cover", "background", "icon"]) {
    const file = join(dir, game[kind]);
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, Buffer.from(v[kind].split(",")[1], "base64"));
  }
}
console.log("drew", games.length);
ws.close();
process.exit(0);
