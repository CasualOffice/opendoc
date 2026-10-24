// docs/179 §4.3: what handing an RGBA page to a canvas costs in Chromium —
// new ImageData + putImageData, then a one-pixel getImageData so the upload is
// not deferred past the timer — and whether WebGPU / WebGL2 are available.
// Needs `playwright-core` resolvable (NODE_PATH) and a Playwright Chromium in
// ~/Library/Caches/ms-playwright: `node benchmarks/render-wasm/blit.mjs`.
import { chromium } from "playwright-core";
import { readdirSync } from "node:fs";
const root = `${process.env.HOME}/Library/Caches/ms-playwright`;
const dir = readdirSync(root).filter((d) => d.startsWith("chromium-")).sort().pop();
const exe = `${root}/${dir}/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`;
const browser = await chromium.launch({ executablePath: exe, args: ["--enable-unsafe-webgpu", "--enable-gpu"] , headless: true});
const page = await browser.newPage();
const out = await page.evaluate(async () => {
  const sizes = [[816, 1056], [1632, 2112], [3264, 4224]];
  const rows = [];
  for (const [w, h] of sizes) {
    const rgba = new Uint8ClampedArray(w * h * 4).fill(200);
    const samples = [];
    for (let i = 0; i < 8; i++) {
      const c = document.createElement("canvas");
      c.width = w; c.height = h;
      document.body.appendChild(c);
      const ctx = c.getContext("2d");
      const t = performance.now();
      ctx.putImageData(new ImageData(rgba, w, h), 0, 0);
      ctx.getImageData(0, 0, 1, 1);
      samples.push(performance.now() - t);
      c.remove();
    }
    samples.shift();
    samples.sort((a, b) => a - b);
    rows.push({ w, h, ms: samples[3] });
  }
  let webgpu = "absent";
  if (navigator.gpu) {
    const adapter = await navigator.gpu.requestAdapter();
    webgpu = adapter ? `adapter: ${adapter.info?.vendor ?? "?"} ${adapter.info?.architecture ?? ""}` : "no adapter";
  }
  const gl2 = !!document.createElement("canvas").getContext("webgl2");
  return { rows, webgpu, gl2, ua: navigator.userAgent };
});
console.log(JSON.stringify(out, null, 1));
await browser.close();
