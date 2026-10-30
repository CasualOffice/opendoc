// One measurement in a fresh process, so peak memory belongs to one library
// and one task. Run by run.mjs; prints one JSON line.
//
//   node task.mjs <task> <library> <fixture-or-size> <iterations> <deps-dir> <sdk-dir>
//
// Libraries are loaded from <deps-dir>/node_modules (installed by the person
// running the suite, never committed) and OpenDoc from <sdk-dir> (the built
// packages/opendoc). Timings are the median of <iterations> runs after one
// warm-up; memory is the process's peak resident set (`process.resourceUsage`).
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [task, library, input, iterationsText, depsDir, sdkDir] = process.argv.slice(2);
const iterations = Number(iterationsText);
const requireDep = createRequire(join(depsDir, "package.json"));
const load = async (name) => import(pathToFileURL(requireDep.resolve(name)).href);
const sdk = () => import(pathToFileURL(join(sdkDir, "src/node.js")).href);
const sdkDocx = () => import(pathToFileURL(join(sdkDir, "src/docx-node.js")).href);

/** A docx-js program run against either implementation: the same source,
 *  only the import differs, which is the claim being measured. */
function buildProgram(D, paragraphs) {
  const children = [new D.Paragraph({ text: "Benchmark report", heading: D.HeadingLevel.HEADING_1 })];
  for (let i = 0; i < paragraphs; i++) {
    children.push(
      new D.Paragraph({
        children: [
          new D.TextRun(`Paragraph ${i}: the quick brown fox jumps over the lazy dog. `),
          new D.TextRun({ text: "Bold", bold: true }),
          new D.TextRun({ text: " and coloured.", color: "1F4E79" }),
        ],
      }),
    );
  }
  const rows = [];
  for (let r = 0; r < Math.max(10, paragraphs / 10); r++) {
    rows.push(
      new D.TableRow({
        children: [0, 1, 2].map((c) => new D.TableCell({ children: [new D.Paragraph(`r${r}c${c}`)] })),
      }),
    );
  }
  children.push(new D.Table({ rows }));
  return new D.Document({ sections: [{ children }] });
}

function templateData(items) {
  return {
    name: "Ada",
    items: Array.from({ length: items }, (_, i) => ({ title: `Item ${i}`, qty: i % 7, price: (i * 1.25).toFixed(2) })),
  };
}

async function templateBytes(items) {
  // The template is built once with docx-js (a neutral producer) so both
  // engines fill the same bytes.
  const D = await load("docx");
  const doc = new D.Document({
    sections: [
      {
        children: [
          new D.Paragraph("Dear {name},"),
          new D.Paragraph("{#items}"),
          new D.Paragraph("{title}: {qty} at {price}"),
          new D.Paragraph("{/items}"),
        ],
      },
    ],
  });
  void items;
  return new Uint8Array(await D.Packer.toBuffer(doc));
}

async function prepare() {
  const bytes = () => readFileSync(input);
  switch (`${task}/${library}`) {
    case "generate/docx-js": {
      const D = await load("docx");
      return async () => (await D.Packer.toBuffer(buildProgram(D, Number(input)))).length;
    }
    case "generate/opendoc": {
      const D = await sdkDocx();
      return async () => (await D.Packer.toBuffer(buildProgram(D, Number(input)))).length;
    }
    case "html/mammoth": {
      const mammoth = await load("mammoth");
      const b = bytes();
      return async () => (await mammoth.convertToHtml({ buffer: b })).value.length;
    }
    case "html/opendoc": {
      const { convertToHtml } = await sdk();
      const b = bytes();
      return async () => convertToHtml({ buffer: b }).value.length;
    }
    case "text/mammoth": {
      const mammoth = await load("mammoth");
      const b = bytes();
      return async () => (await mammoth.extractRawText({ buffer: b })).value.length;
    }
    case "text/opendoc": {
      const { extractRawText } = await sdk();
      const b = bytes();
      return async () => extractRawText({ buffer: b }).value.length;
    }
    case "template/docxtemplater": {
      const PizZip = (await load("pizzip")).default;
      const Docxtemplater = (await load("docxtemplater")).default;
      const t = await templateBytes();
      const data = templateData(Number(input));
      return async () => {
        const doc = new Docxtemplater(new PizZip(t), { paragraphLoop: true, linebreaks: true });
        doc.render(data);
        return doc.getZip().generate({ type: "uint8array" }).length;
      };
    }
    case "template/opendoc": {
      const { Template } = await sdk();
      const t = await templateBytes();
      const data = templateData(Number(input));
      return async () => {
        const doc = new Template(t);
        doc.render(data);
        const out = doc.toBuffer().length;
        doc.free();
        return out;
      };
    }
    case "pdf/opendoc": {
      const { Document } = await sdk();
      const b = bytes();
      return async () => {
        const doc = Document.open(b);
        const out = doc.toPdf().length;
        doc.free();
        return out;
      };
    }
    case "open/opendoc": {
      const { Document } = await sdk();
      const b = bytes();
      return async () => {
        const doc = Document.open(b);
        const out = doc.paragraphs.length;
        doc.free();
        return out;
      };
    }
    default:
      throw new Error(`no task ${task}/${library}`);
  }
}

const run = await prepare();
let output = await run(); // warm-up
const times = [];
for (let i = 0; i < iterations; i++) {
  const start = process.hrtime.bigint();
  output = await run();
  times.push(Number(process.hrtime.bigint() - start) / 1e6);
}
times.sort((a, b) => a - b);
const median = times[Math.floor(times.length / 2)];
// maxRSS is in kilobytes on every platform Node supports.
const peakMB = process.resourceUsage().maxRSS / 1024;
process.stdout.write(`${JSON.stringify({ ms: Number(median.toFixed(2)), peakMB: Number(peakMB.toFixed(1)), output })}\n`);
