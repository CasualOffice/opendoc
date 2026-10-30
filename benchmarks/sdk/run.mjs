#!/usr/bin/env node
// The SDK benchmark and conformance suite (docs/176 section 12.10).
//
// The same fixtures go through OpenDoc and through every permissive competitor
// installed on this machine; it measures speed, peak memory, page-count
// agreement and round-trip loss, and writes benchmarks/sdk/results.json. The
// published table is rendered from that file by report.mjs and never typed.
//
//   node benchmarks/sdk/run.mjs --deps DIR [--python PY] [--java-cp CP] [--soffice PATH]
//                               [--iterations 5] [--out benchmarks/sdk/results.json]
//
// --deps is a directory whose node_modules holds docx, mammoth, docxtemplater
// and pizzip (npm install them there; nothing is committed). Build the SDK
// first (packages/opendoc/build.sh). Optional competitors are skipped, and the
// results say so, when absent. Run it on an otherwise idle machine: the owner's
// rule is that benchmarks never run beside a build.
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { cpus, platform, arch, release, tmpdir, totalmem } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..");
const sdkDir = join(repo, "packages", "opendoc");

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const depsDir = opt("--deps");
if (!depsDir) throw new Error("--deps DIR is required (a directory with docx, mammoth, docxtemplater, pizzip installed)");
const iterations = Number(opt("--iterations", "5"));
const outPath = opt("--out", join(here, "results.json"));
const python = opt("--python");
const javaCp = opt("--java-cp");
const soffice = opt("--soffice");

/** The fixtures: every licensed DOCX in the repository corpus plus sample.docx. */
const manifest = JSON.parse(readFileSync(join(repo, "fixtures", "manifest.json"), "utf8"));
const fixtures = [
  { name: "sample.docx", path: join(repo, "sample.docx") },
  ...manifest.fixtures
    .filter((f) => f.path.startsWith("corpus/") && f.path.endsWith(".docx"))
    .map((f) => ({ name: basename(f.path), path: join(repo, "fixtures", f.path) })),
];

function task(name, library, input) {
  const result = spawnSync(
    process.execPath,
    [join(here, "task.mjs"), name, library, String(input), String(iterations), depsDir, sdkDir],
    { encoding: "utf8", maxBuffer: 1 << 26 },
  );
  if (result.status !== 0) return { error: (result.stderr || "").trim().split("\n").slice(-3).join(" ") };
  return JSON.parse(result.stdout.trim().split("\n").pop());
}

function versionOf(name) {
  try {
    const require = createRequire(join(depsDir, "package.json"));
    let dir = dirname(require.resolve(name));
    for (let i = 0; i < 5; i++) {
      try {
        const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
        if (pkg.name === name) return pkg.version;
      } catch {}
      dir = dirname(dir);
    }
  } catch {}
  return null;
}

const sdk = await import(pathToFileURL(join(sdkDir, "src", "node.js")).href);
const results = {
  schema: 1,
  generatedBy: "benchmarks/sdk/run.mjs",
  environment: {
    platform: `${platform()} ${arch()} ${release()}`,
    cpu: cpus()[0]?.model ?? "unknown",
    cores: cpus().length,
    memoryGB: Math.round(totalmem() / 2 ** 30),
    node: process.version,
    iterations,
  },
  versions: {
    opendoc: sdk.version(),
    docx: versionOf("docx"),
    mammoth: versionOf("mammoth"),
    docxtemplater: versionOf("docxtemplater"),
    pizzip: versionOf("pizzip"),
  },
  speed: [],
  fidelity: [],
  roundTrip: [],
  notes: [],
};

const log = (message) => process.stderr.write(`${message}\n`);

if (python) {
  const v = spawnSync(python, ["-c", "import docx; print(docx.__version__)"], { encoding: "utf8" });
  results.versions["python-docx"] = v.status === 0 ? v.stdout.trim() : null;
}

// --- speed and memory -----------------------------------------------------------

const pairs = [
  { task: "generate", libraries: ["docx-js", "opendoc"], inputs: [100, 1000, 5000], label: (n) => `${n} paragraphs + table` },
  { task: "template", libraries: ["docxtemplater", "opendoc"], inputs: [100, 1000], label: (n) => `${n}-item loop` },
  { task: "html", libraries: ["mammoth", "opendoc"], inputs: ["sample", "scale"], label: (n) => n },
  { task: "text", libraries: ["mammoth", "opendoc"], inputs: ["sample", "scale"], label: (n) => n },
  { task: "pdf", libraries: ["opendoc"], inputs: ["sample", "scale"], label: (n) => n },
];

// A large document, written by docx-js (a neutral producer) at run time.
const scratch = mkdtempSync(join(tmpdir(), "opendoc-bench-"));
const scalePath = join(scratch, "scale.docx");
{
  const require = createRequire(join(depsDir, "package.json"));
  const D = await import(pathToFileURL(require.resolve("docx")).href);
  const children = [];
  for (let i = 0; i < 3000; i++) {
    if (i % 100 === 0) children.push(new D.Paragraph({ text: `Section ${i / 100}`, heading: D.HeadingLevel.HEADING_1 }));
    children.push(new D.Paragraph({ children: [new D.TextRun(`Line ${i}: lorem ipsum dolor sit amet, consectetur adipiscing elit. `), new D.TextRun({ text: "Emphasis", italics: true })] }));
  }
  writeFileSync(scalePath, await D.Packer.toBuffer(new D.Document({ sections: [{ children }] })));
}
const named = { sample: join(repo, "sample.docx"), scale: scalePath };
results.notes.push("`scale` is 3,000 paragraphs and 30 headings written by docx-js at run time.");

for (const pair of pairs) {
  for (const input of pair.inputs) {
    const row = { task: pair.task, input: pair.label(input) };
    for (const library of pair.libraries) {
      log(`${pair.task} ${input} ${library}`);
      row[library] = task(pair.task, library, named[input] ?? input);
    }
    results.speed.push(row);
  }
}

// Python: the same python-docx program against python-docx and against the
// opendoc wheel (both installed in the interpreter given by --python).
function pythonTask(name, library, input) {
  const result = spawnSync(python, [join(here, "python_task.py"), name, library, String(input), String(iterations)], {
    encoding: "utf8",
  });
  if (result.status !== 0) return { error: (result.stderr || "").trim().split("\n").pop() };
  return JSON.parse(result.stdout.trim().split("\n").pop());
}
if (python) {
  for (const [name, inputs, label] of [
    ["generate", [100, 1000, 5000], (n) => `${n} paragraphs + table (Python)`],
    ["read", ["sample", "scale"], (n) => `${n} (Python)`],
  ]) {
    for (const input of inputs) {
      const row = { task: `py-${name}`, input: label(input) };
      for (const library of ["python-docx", "opendoc"]) {
        log(`py-${name} ${input} ${library}`);
        row[library] = pythonTask(name, library, named[input] ?? input);
      }
      results.speed.push(row);
    }
  }
}

// Java: the same POI-style program against Apache POI XWPF and against the
// OpenDoc binding (POI's naming over the C ABI). --java-cp is a directory of
// POI's jars; --java-lib the built libopendoc_ffi. Peak RSS comes from the OS
// (`/usr/bin/time`), so it includes the JVM itself.
const javaLib = opt("--java-lib");
if (javaCp && javaLib) {
  const classes = join(scratch, "java-classes");
  const jars = readdirSync(javaCp)
    .filter((f) => f.endsWith(".jar"))
    .map((f) => join(javaCp, f));
  const sources = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith(".java")) sources.push(path);
    }
  };
  walk(join(repo, "bindings", "java", "src", "main", "java"));
  sources.push(join(here, "java", "Bench.java"));
  const sep = platform() === "win32" ? ";" : ":";
  const cp = [...jars, classes].join(sep);
  execFileSync("javac", ["--release", "22", "-d", classes, "-cp", jars.join(sep), ...sources], { stdio: "inherit" });
  const timeFlag = platform() === "darwin" ? "-l" : "-v";
  const javaTask = (name, library, input) => {
    const run = spawnSync(
      "/usr/bin/time",
      [timeFlag, "java", "--enable-native-access=ALL-UNNAMED", `-Dopendoc.library=${javaLib}`, "-cp", cp, "Bench", name, library, String(input), String(iterations)],
      { encoding: "utf8" },
    );
    if (run.status !== 0) return { error: run.stderr.trim().split("\n").slice(-2).join(" ") };
    const result = JSON.parse(run.stdout.trim().split("\n").pop());
    const darwin = /(\d+)\s+maximum resident set size/.exec(run.stderr);
    const linux = /Maximum resident set size \(kbytes\): (\d+)/.exec(run.stderr);
    result.peakMB = darwin ? Number((Number(darwin[1]) / 2 ** 20).toFixed(1)) : linux ? Number((Number(linux[1]) / 1024).toFixed(1)) : null;
    return result;
  };
  results.versions.poi = (/poi-(\d[\w.-]*)\.jar/.exec(jars.join(" ")) ?? [])[1] ?? null;
  {
    const v = spawnSync("java", ["-version"], { encoding: "utf8" });
    results.versions.java = (v.stderr || v.stdout).trim().split("\n")[0];
  }
  for (const [name, inputs, label] of [
    ["generate", [100, 1000], (n) => `${n} paragraphs + table (Java)`],
    ["read", ["sample", "scale"], (n) => `${n} (Java)`],
  ]) {
    for (const input of inputs) {
      const row = { task: `java-${name}`, input: label(input) };
      for (const library of ["poi", "opendoc"]) {
        log(`java-${name} ${input} ${library}`);
        row[library] = javaTask(name, library, named[input] ?? input);
      }
      results.speed.push(row);
    }
  }
  results.notes.push("Java peak memory is the whole JVM process's maximum resident set (`/usr/bin/time`), for both libraries.");
}

// LibreOffice headless: DOCX to PDF, batch and single, wall time including its
// own start-up (that is what a server pays per conversion request).
if (soffice) {
  const out = join(scratch, "lo");
  const timeIt = (files) => {
    const start = process.hrtime.bigint();
    execFileSync(soffice, ["--headless", "--convert-to", "pdf", "--outdir", out, ...files], { stdio: "ignore" });
    return Number(process.hrtime.bigint() - start) / 1e6;
  };
  timeIt([named.sample]); // warm the profile
  const sampleMs = timeIt([named.sample]);
  const scaleMs = timeIt([named.scale]);
  for (const [input, ms] of [["sample", sampleMs], ["scale", scaleMs]]) {
    const row = results.speed.find((r) => r.task === "pdf" && r.input === input);
    row.libreoffice = { ms: Number(ms.toFixed(0)), peakMB: null };
  }
  results.versions.libreoffice = execFileSync(soffice, ["--version"], { encoding: "utf8" }).trim().split("\n")[0];
  results.notes.push("LibreOffice is timed as one `soffice --headless --convert-to pdf` process per document, start-up included; its memory is not measured.");
} else {
  results.notes.push("LibreOffice was not given (--soffice), so its PDF timings and page counts are absent.");
}

// --- fidelity: page count agreement with LibreOffice --------------------------------

function pdfPages(bytes) {
  return (bytes.toString("latin1").match(/\/Type\s*\/Page[^s]/g) ?? []).length;
}

for (const fixture of fixtures) {
  const doc = sdk.Document.open(readFileSync(fixture.path));
  const row = { fixture: fixture.name, opendocPages: doc.pageCount };
  doc.free();
  if (soffice) {
    const out = join(scratch, "lo-pages");
    execFileSync(soffice, ["--headless", "--convert-to", "pdf", "--outdir", out, fixture.path], { stdio: "ignore" });
    row.libreofficePages = pdfPages(readFileSync(join(out, fixture.name.replace(/\.docx$/, ".pdf"))));
  }
  results.fidelity.push(row);
}

// --- round trip: open, save, reopen --------------------------------------------------

function zipParts(bytes) {
  // Central-directory names, enough to compare part lists without a zip library.
  const names = new Set();
  const view = Buffer.from(bytes);
  for (let i = 0; i + 46 < view.length; i++) {
    if (view.readUInt32LE(i) === 0x02014b50) {
      const nameLength = view.readUInt16LE(i + 28);
      names.add(view.toString("utf8", i + 46, i + 46 + nameLength));
    }
  }
  return names;
}

function texts(bytes) {
  const doc = sdk.Document.open(bytes);
  const t = doc.text;
  doc.free();
  return t;
}

for (const fixture of fixtures) {
  const input = readFileSync(fixture.path);
  const before = zipParts(input);
  const doc = sdk.Document.open(input);
  const preserve = doc.save("docx", { mode: "preserve" });
  const semantic = doc.save("docx", { mode: "semantic" });
  const reference = doc.text;
  doc.free();
  const kept = (bytes) => [...before].filter((p) => zipParts(bytes).has(p)).length;
  const row = {
    fixture: fixture.name,
    parts: before.size,
    opendoc: {
      preserve: { partsKept: kept(preserve.bytes), losses: preserve.report.losses.length, textSame: texts(preserve.bytes) === reference },
      semantic: { partsKept: kept(semantic.bytes), losses: semantic.report.losses.length, textSame: texts(semantic.bytes) === reference },
    },
  };
  if (python) {
    const outFile = join(scratch, `py-${fixture.name}`);
    const run = spawnSync(python, [join(here, "python_docx_roundtrip.py"), fixture.path, outFile], { encoding: "utf8" });
    if (run.status === 0) {
      const bytes = readFileSync(outFile);
      row["python-docx"] = { partsKept: kept(bytes), losses: null, textSame: texts(bytes) === reference };
    } else {
      row["python-docx"] = { error: run.stderr.trim().split("\n").pop() };
    }
  }
  results.roundTrip.push(row);
}
results.notes.push(
  "Round trip: an empty `docProps/custom.xml` (no properties) is not rewritten by an OpenDoc save, which `partsKept` counts as a dropped part although nothing is lost. `partsKept` counts the input's package parts present in the output; `textSame` re-opens the output with OpenDoc and compares the body text; `losses` is OpenDoc's own save report (no other library reports loss).",
);

rmSync(scratch, { recursive: true, force: true });
writeFileSync(outPath, `${JSON.stringify(results, null, 2)}\n`);
log(`wrote ${outPath}`);
