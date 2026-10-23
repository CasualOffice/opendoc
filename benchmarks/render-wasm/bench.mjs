// node bench.mjs <module.wasm> <doc.docx> <label>
import { readFileSync } from "node:fs";
const [wasmPath, docPath, label] = process.argv.slice(2);
const bytes = readFileSync(wasmPath);
const mod = await WebAssembly.compile(bytes);
const imports = {};
for (const imp of WebAssembly.Module.imports(mod)) {
  imports[imp.module] ??= {};
  imports[imp.module][imp.name] = () => { throw new Error(`import ${imp.module}.${imp.name}`); };
}
const { exports } = await WebAssembly.instantiate(mod, imports);
const doc = readFileSync(docPath);
const ptr = exports.input_ptr(doc.length);
new Uint8Array(exports.memory.buffer, ptr, doc.length).set(doc);
const pages = exports.load();
const median = (f) => {
  f();
  const s = [];
  for (let i = 0; i < 7; i++) { const t = performance.now(); f(); s.push(performance.now() - t); }
  s.sort((a, b) => a - b);
  return s[3];
};
const want = process.env.PAGES ? process.env.PAGES.split(",").map(Number) : [...Array(pages).keys()];
for (const p of want) {
  for (const dpi of [96, 192, 384]) {
    const cpu = median(() => exports.render_cpu(p, dpi));
    const sparse = exports.render_sparse ? median(() => exports.render_sparse(p, dpi)) : NaN;
    console.log(`${label},${p + 1},${dpi},${cpu.toFixed(2)},${sparse.toFixed(2)}`);
  }
}
