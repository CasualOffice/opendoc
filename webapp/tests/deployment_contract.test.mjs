// The deployment contract: `docker-compose.yml` and the two Dockerfiles against the
// interface `server/src/main.rs` actually has (`docs/162` §15, `109` RM-11).
//
// WHY THIS FILE EXISTS. A guide and a compose file rot silently, and they rot in a
// way nothing else in this repository can see: the relay's CLI is three literal
// patterns matched inside `main()`, so renaming a subcommand, reordering the
// arguments or dropping a role leaves every Rust test green and ships a compose
// file that cannot start. `SKILL.md` §8 says counts in docs must be derived, and §9
// says a published claim must be true and sourced — a deployment guide is a
// published claim, and the numbers in it are exactly the kind that drift.
//
// So nothing here is a literal typed twice. Every assertion reads BOTH sides:
//
//   * `server/src/main.rs` for the subcommands, their argument order and arity,
//     and the five roles `open_room_role` matches;
//   * `crates/casual-doc-transaction/src/presence.rs` for `MAX_PARTICIPANTS`;
//   * `rust-toolchain.toml` for the pinned channel;
//   * `.github/workflows/ci.yml` for the wasm-pack version and the Node major;
//   * `docker-compose.yml`, both Dockerfiles and `deploy/editor-nginx.conf`;
//   * `docs/162` for what the guide publishes.
//
// It was driven RED before it was trusted, per `SKILL.md` §4 — the mutations and
// their output are in the commit message. The shapes that must fail are: a renamed
// subcommand, a reordered `serve` argument list, a sixth role, a port the document
// and the compose file disagree about, a `CMD` added to the relay image, and a
// `sub_filter` in the nginx config.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = dirname(dirname(fileURLToPath(import.meta.url)));
const REPO = dirname(WEBAPP);

const read = (...parts) => readFileSync(join(REPO, ...parts), "utf8");

const MAIN_RS = read("server", "src", "main.rs");
const PRESENCE_RS = read("crates", "casual-doc-transaction", "src", "presence.rs");
const TOOLCHAIN = read("rust-toolchain.toml");
const CI = read(".github", "workflows", "ci.yml");
const COMPOSE_TEXT = read("docker-compose.yml");
const RELAY_DOCKERFILE = read("Dockerfile.relay");
const EDITOR_DOCKERFILE = read("Dockerfile.editor");
const NGINX_SOURCE = read("deploy", "editor-nginx.conf");
// The DIRECTIVES, with the comments stripped. This file documents why it must not
// rewrite a font URL, so the explanation names `sub_filter` and a Google Fonts
// host — and the first run of this guard failed on its own comment, which is the
// cheapest possible reminder that a guard must assert the guarantee and not the
// text.
const NGINX = NGINX_SOURCE.split("\n")
  .filter((line) => !/^\s*#/.test(line))
  .join("\n");
const GUIDE = read("docs", "162-DEPLOYMENT-CONTAINERS-AND-CONFIGURATION.md");

// ---------------------------------------------------------------------------
// The binary's real interface, parsed out of `main()`.
//
// `main()` collects the arguments into a `Vec<String>` and matches the slice
// against literal patterns:
//
//     ["create", journal] => …
//     ["inspect", journal] => …
//     ["serve", journal, address, role] => …
//
// so the pattern list IS the interface, including the ORDER of the bindings. The
// binding names are read as well as the arity, because "serve journal addr role"
// and "serve addr journal role" have the same arity and only one of them starts.
function binaryInterface() {
  const patterns = [...MAIN_RS.matchAll(/\[\s*"([a-z]+)"\s*((?:,\s*\w+\s*)*)\]\s*=>/g)];
  assert.ok(
    patterns.length >= 3,
    "could not find the argument-slice match arms in server/src/main.rs — the CLI is " +
      "parsed by matching a &[&str] against literal patterns, and this guard reads those " +
      "patterns. If main() stopped doing that, re-derive this helper from whatever " +
      "replaced it rather than deleting the assertion",
  );
  const commands = new Map();
  for (const [, name, rest] of patterns) {
    const operands = rest
      .split(",")
      .map((part) => part.trim())
      .filter(Boolean);
    commands.set(name, operands);
  }
  return commands;
}

/** The roles `open_room_role` accepts, in the order it matches them. */
function binaryRoles() {
  const body = MAIN_RS.split("fn open_room_role")[1];
  assert.ok(body, "server/src/main.rs must define open_room_role");
  const arms = [...body.split("fn ")[0].matchAll(/"([a-z]+)"\s*=>\s*Capabilities::/g)].map(
    (match) => match[1],
  );
  assert.ok(arms.length > 0, "no role arms found in open_room_role");
  return arms;
}

// ---------------------------------------------------------------------------
// A deliberately small YAML reader for the subset `docker-compose.yml` is written
// in: block mappings, block sequences, comments, two-space indentation. Not a
// general parser, and it is not trying to be one — it throws on anything it does
// not understand so that a reshaped compose file fails loudly here instead of
// being half-read. (`docker compose config` would be the real parser, and it needs
// a daemon the unit suite does not have.)
function parseYaml(text) {
  // Comments and blank lines first, so the look-ahead below cannot be fooled by
  // one. The compose file is heavily commented on purpose — the comments are
  // where the design decisions are recorded — so this is not an edge case.
  const lines = text
    .split("\n")
    .map((line) => (/^\s*#/.test(line) ? "" : line))
    .map((line, index) => ({ index, raw: line }))
    .filter((line) => line.raw.trim());

  const indentOf = (line) => line.raw.match(/^ */)[0].length;

  // Whether a `key:` with no value opens a mapping or a sequence is decided by
  // its FIRST CHILD, so it is looked up rather than guessed. An earlier draft
  // pushed a mapping and let a `- ` item fail to find an array parent, which is
  // the failure this look-ahead removes.
  const childIsSequence = (at) => {
    const indent = indentOf(lines[at]);
    for (let i = at + 1; i < lines.length; i += 1) {
      const next = indentOf(lines[i]);
      if (next <= indent) return false;
      return lines[i].raw.trim().startsWith("- ");
    }
    return false;
  };

  const root = {};
  const stack = [{ indent: -1, node: root }];
  lines.forEach((line, at) => {
    const indent = indentOf(line);
    const body = line.raw.trim();
    while (stack.length > 1 && indent <= stack[stack.length - 1].indent) stack.pop();
    const parent = stack[stack.length - 1].node;

    if (body.startsWith("- ")) {
      assert.ok(Array.isArray(parent), `a sequence item outside a sequence: ${line.raw}`);
      parent.push(unquote(body.slice(2).trim()));
      return;
    }
    const match = body.match(/^([A-Za-z0-9_.-]+):\s*(.*)$/);
    assert.ok(match, `deployment_contract: unparsed compose line: ${line.raw}`);
    const [, key, value] = match;
    assert.ok(!Array.isArray(parent), `a mapping key inside a sequence: ${line.raw}`);
    if (value === "") {
      const child = childIsSequence(at) ? [] : {};
      parent[key] = child;
      stack.push({ indent, node: child });
      return;
    }
    parent[key] = unquote(value);
  });
  return root;
}

const unquote = (value) =>
  /^".*"$/.test(value) || /^'.*'$/.test(value) ? value.slice(1, -1) : value;

// The parser's own guard. A hand-rolled reader that silently mis-parses would make
// every assertion below vacuous, which is the `docs/105` CQ-003 failure mode.
function compose() {
  const parsed = parseYaml(COMPOSE_TEXT);
  assert.ok(parsed.services, "docker-compose.yml must declare services");
  assert.ok(
    Object.keys(parsed.services).length >= 2,
    "the compose file must carry the editor and at least one relay service",
  );
  return parsed;
}

/** Every service whose image is the relay image, with its command. */
function relayServices() {
  const services = compose().services;
  const out = new Map();
  for (const [name, service] of Object.entries(services)) {
    if (service.dockerfile === "Dockerfile.relay" || service.build?.dockerfile === "Dockerfile.relay") {
      assert.ok(
        Array.isArray(service.command),
        `compose service ${name} must give its command as a list, so the ARGUMENT ORDER is ` +
          "readable; a shell string would hide it from this guard",
      );
      out.set(name, service.command);
    }
  }
  assert.ok(out.size > 0, "no compose service builds Dockerfile.relay");
  return out;
}

/** `"8099:8080"` -> `{ host: 8099, container: 8080 }`. */
function port(mapping) {
  const match = String(mapping).match(/^(\d+):(\d+)$/);
  assert.ok(match, `a published port must read "host:container", got ${mapping}`);
  return { host: Number(match[1]), container: Number(match[2]) };
}

// ===========================================================================

test("every repository-root file build.sh reads is in the editor image's context", () => {
  // THE BUG THIS EXISTS FOR, because it is the expensive kind, and it happened
  // TWICE before this guard was wide enough.
  //
  // `Dockerfile.editor` copies an ENUMERATED list of inputs rather than
  // `COPY . .` — correctly, because this repository's build context holds the
  // owner's untracked personal documents and `.dockerignore` is an ALLOWLIST for
  // the same reason. The cost of that choice is that a generator reaching for a
  // new file at the repository root is excluded twice over, and nothing says so
  // until the build fails:
  //
  //   build 1, 19 minutes in, after an 18m50s wasm compile:
  //     Error: ENOENT: no such file or directory, open '/src/docker-compose.yml'
  //       at composeServices (webapp/tools/build-embed-docs.mjs:556)
  //   build 2, 7 minutes in, one generator later:
  //     Error: ENOENT: no such file or directory, open '/src/CONTRIBUTING.md'
  //       at describe (webapp/tools/build-doc-pages.mjs:760)
  //
  // The first version of this guard read ONE generator's sources and would have
  // caught only the first. So it now reads EVERY generator `build.sh` runs, in
  // both the shapes they use to name a repository-root file:
  //
  //   * `join(REPO, "x", …)` — a path built from the repository root;
  //   * `source: "x"` — `build-doc-pages.mjs`'s published-document list, which is
  //     how `CONTRIBUTING.md` and `SECURITY.md` become reference pages.
  //
  // Derived, so the next generator to reach for a root file is covered on
  // arrival rather than after a wasm compile.
  //
  // WHICH generators is itself read out of `build.sh`, not globbed from
  // `tools/`. The first version globbed, and flagged `en-US-supplement.txt` —
  // a real root file, read by `build-dictionary.mjs`, which `build.sh` does not
  // run. A guard that reports a file the build does not need is a guard somebody
  // will switch off, so the question it asks is narrowed to exactly the scripts
  // the image executes.
  const buildScript = readFileSync(join(WEBAPP, "build.sh"), "utf8");
  const invoked = [...buildScript.matchAll(/tools\/([\w-]+\.mjs)/g)].map((match) => match[1]);
  assert.ok(
    invoked.length >= 4,
    `read only ${invoked.length} generator(s) out of build.sh — the scan is reading the ` +
      "wrong thing",
  );
  const present = new Set(readdirSync(join(WEBAPP, "tools")));
  for (const name of invoked) {
    assert.ok(present.has(name), `build.sh runs tools/${name}, which does not exist`);
  }
  const generators = [...new Set(invoked)]
    .map((name) => readFileSync(join(WEBAPP, "tools", name), "utf8"))
    .join("\n");

  const fromJoin = [...generators.matchAll(/join\(REPO,\s*((?:"[^"]+"\s*,?\s*)+)\)/g)].map(
    (match) => [...match[1].matchAll(/"([^"]+)"/g)].map((part) => part[1]).join("/"),
  );
  const fromPublished = [...generators.matchAll(/source:\s*"([^"]+)"/g)].map((m) => m[1]);
  // `build.sh` itself stages three files by name, with the same `$repo` prefix.
  const fromScript = [...buildScript.matchAll(/\$repo\/([\w./-]+)/g)].map((m) => m[1]);

  const roots = [
    ...new Set([...fromJoin, ...fromPublished, ...fromScript].map((p) => p.split("/")[0])),
  ]
    .filter((root) => root && root !== "webapp" && root !== "target")
    .sort();

  // The guard's own guard: if the scan stops finding the two files that caused
  // the failures, it has stopped reading the generators and is asserting nothing.
  for (const expected of ["docker-compose.yml", "CONTRIBUTING.md", "docs", "packages"]) {
    assert.ok(
      roots.includes(expected),
      `the scan no longer finds ${expected} among build.sh's repository-root inputs, so it ` +
        `is reading the wrong thing — it found: ${roots.join(", ")}`,
    );
  }

  const ignore = read(".dockerignore");
  const copied = [...EDITOR_DOCKERFILE.matchAll(/^COPY\s+(?!--from)([^\n]+)$/gm)].flatMap(
    (match) => match[1].trim().split(/\s+/).slice(0, -1),
  );

  const missing = [];
  for (const root of roots) {
    if (!copied.includes(root)) missing.push(`${root}: no COPY in Dockerfile.editor`);
    // The allowlist: `*` excludes everything, so each entry needs its own `!`.
    if (!new RegExp(`^!${root.replace(/[.]/g, "\\.")}/?$`, "m").test(ignore)) {
      missing.push(`${root}: not allowlisted in .dockerignore`);
    }
  }
  assert.deepEqual(
    missing,
    [],
    "webapp/build.sh reads these from the repository root, so the editor image cannot be " +
      "built without them — and the build discovers it AFTER the wasm compile, not before",
  );
});

test("the compose file's parser read something real", () => {
  const parsed = compose();
  assert.ok(parsed.services.editor, "there must be an `editor` service");
  assert.ok(parsed.volumes, "the relay's journal volume must be declared");
});

test("every relay command is a subcommand the binary matches, with its arity", () => {
  const commands = binaryInterface();
  for (const [service, command] of relayServices()) {
    const [subcommand, ...operands] = command;
    assert.ok(
      commands.has(subcommand),
      `compose service ${service} runs \`${subcommand}\`, which server/src/main.rs does not ` +
        `match. It matches: ${[...commands.keys()].join(", ")}`,
    );
    assert.equal(
      operands.length,
      commands.get(subcommand).length,
      `compose service ${service} passes ${operands.length} operand(s) to \`${subcommand}\`, ` +
        `which takes ${commands.get(subcommand).length} ` +
        `(${commands.get(subcommand).join(" ")}) — a wrong arity exits 2 with the usage line`,
    );
  }
});

test("the argument ORDER matches the binary's bindings, not just the count", () => {
  // `["serve", journal, address, role]`. The binding names are the contract: a
  // compose file that passed the address first would have the right arity and
  // would fail at `TcpListener::bind`.
  const commands = binaryInterface();
  const shape = (operand) => {
    if (/^\d+\.\d+\.\d+\.\d+:\d+$|^\[?[0-9a-f:]*\]?:\d+$/i.test(operand)) return "address";
    if (operand.includes("/")) return "journal";
    return "role";
  };
  for (const [service, command] of relayServices()) {
    const [subcommand, ...operands] = command;
    const expected = commands.get(subcommand);
    operands.forEach((operand, index) => {
      assert.equal(
        shape(operand),
        expected[index],
        `compose service ${service}: \`${subcommand}\` argument ${index + 1} is ` +
          `${shape(operand)}-shaped (${operand}), but main.rs binds it as ` +
          `\`${expected[index]}\` — the positions are the whole interface`,
      );
    });
  }
});

test("the roles named in compose and in docs/162 are exactly the five the binary accepts", () => {
  const roles = binaryRoles();
  assert.equal(
    new Set(roles).size,
    roles.length,
    `open_room_role matches a role twice: ${roles.join(", ")}`,
  );

  // The compose file names every role it knows in the usage comment on the
  // `serve` command, and runs exactly one of them.
  const inCompose = new Set(
    [...COMPOSE_TEXT.matchAll(/<(viewer\|[a-z|]+)>/g)].flatMap((m) => m[1].split("|")),
  );
  assert.deepEqual(
    [...inCompose].sort(),
    [...roles].sort(),
    "the role list in docker-compose.yml's usage comment is not the set open_room_role " +
      "matches — a role that exists in one and not the other is either an unreachable " +
      "option or a documented permission the binary refuses",
  );

  // The guide's role table, read by heading so a role mentioned in prose
  // elsewhere cannot stand in for a row.
  const section = GUIDE.split("### 5.3")[1];
  assert.ok(section, "docs/162 must carry a §5.3 with the role table");
  const inGuide = new Set(
    section
      .split("\n### ")[0]
      .split("\n")
      .filter((line) => line.startsWith("| `"))
      .map((line) => line.match(/^\| `([a-z]+)`/)?.[1])
      .filter(Boolean),
  );
  assert.deepEqual(
    [...inGuide].sort(),
    [...roles].sort(),
    "docs/162 §5.3's role table is not the set open_room_role matches. Understating is " +
      "also false (SKILL §9.3): the table enumerates the roles, so a missing row is a " +
      "capability the guide hides and an extra row is one it invents",
  );

  for (const [service, command] of relayServices()) {
    if (command[0] !== "serve") continue;
    assert.ok(
      roles.includes(command[command.length - 1]),
      `compose service ${service} serves with role \`${command[command.length - 1]}\`, which ` +
        "is not one the binary accepts",
    );
  }

  // The COUNT in §2's derived-constants table, which is the one a reader quotes
  // without opening the role table. Every other row in that table is asserted
  // against its source below; this one closes it.
  assert.equal(
    Number(statedConstant("Roles")),
    roles.length,
    "docs/162 §2 states a number of roles that open_room_role does not match",
  );
});

test("the relay's port agrees across compose, the serve address, EXPOSE and docs/162", () => {
  const services = compose().services;
  const relay = Object.entries(services).find(
    ([, s]) => s.build?.dockerfile === "Dockerfile.relay" && Array.isArray(s.ports),
  );
  assert.ok(relay, "a relay service must publish a port");
  const published = port(relay[1].ports[0]);

  const served = relay[1].command.find((argument) => /:\d+$/.test(argument));
  assert.ok(served, "the serve command must carry a listen address");
  assert.equal(
    Number(served.split(":").pop()),
    published.container,
    "the relay binds a different port from the one compose publishes inside the container",
  );

  const exposed = RELAY_DOCKERFILE.match(/^EXPOSE\s+(\d+)/m);
  assert.ok(exposed, "Dockerfile.relay must EXPOSE its port");
  assert.equal(
    Number(exposed[1]),
    published.container,
    "Dockerfile.relay's EXPOSE and the port the relay actually binds disagree",
  );

  // The healthcheck has to test the port that is served, or it tests nothing.
  const health = RELAY_DOCKERFILE.match(/dev\/tcp\/127\.0\.0\.1\/(\d+)/);
  assert.ok(health, "Dockerfile.relay's healthcheck must connect to the served port");
  assert.equal(
    Number(health[1]),
    published.container,
    "Dockerfile.relay's healthcheck connects to a port the relay does not bind, so it " +
      "would report the container unhealthy for ever",
  );

  assert.equal(
    Number(statedConstant("Host and container port for the relay")),
    published.host,
    "docs/162 §2 states a relay port the compose file does not publish",
  );
});

test("the editor's port agrees across compose, nginx, EXPOSE and docs/162", () => {
  const editor = compose().services.editor;
  assert.ok(Array.isArray(editor.ports), "the editor service must publish a port");
  const published = port(editor.ports[0]);

  const listens = [...NGINX.matchAll(/^\s*listen\s+(?:\[::\]:)?(\d+);/gm)].map((m) =>
    Number(m[1]),
  );
  assert.ok(listens.length > 0, "deploy/editor-nginx.conf must listen on a port");
  for (const listen of listens) {
    assert.equal(
      listen,
      published.container,
      "nginx listens on a port the editor container does not publish",
    );
  }

  const exposed = EDITOR_DOCKERFILE.match(/^EXPOSE\s+(\d+)/m);
  assert.ok(exposed, "Dockerfile.editor must EXPOSE its port");
  assert.equal(Number(exposed[1]), published.container, "Dockerfile.editor's EXPOSE is wrong");

  const health = EDITOR_DOCKERFILE.match(/http:\/\/127\.0\.0\.1:(\d+)\//);
  assert.ok(health, "Dockerfile.editor's healthcheck must fetch over HTTP");
  assert.equal(
    Number(health[1]),
    published.container,
    "Dockerfile.editor's healthcheck asks a port nginx does not listen on",
  );

  assert.equal(
    Number(statedConstant("Host port compose publishes for the editor")),
    published.host,
    "docs/162 §2 states an editor port the compose file does not publish",
  );
  assert.equal(
    Number(statedConstant("Port nginx listens on inside the editor container")),
    published.container,
    "docs/162 §2 states a container port nginx does not listen on",
  );
});

/** One value from docs/162 §2's derived-constants table, by its description. */
function statedConstant(what) {
  const row = GUIDE.split("\n").find((line) => line.startsWith("| ") && line.includes(what));
  assert.ok(row, `docs/162 §2 must carry a derived-constants row for "${what}"`);
  const value = row.match(/^\|\s*`?([^|`]+?)`?\s*\|/);
  assert.ok(value, `could not read the value out of: ${row}`);
  return value[1].trim();
}

test("the journal the compose command names is inside the volume the image declares", () => {
  const declared = RELAY_DOCKERFILE.match(/^VOLUME\s+\["([^"]+)"\]/m);
  assert.ok(
    declared,
    "Dockerfile.relay must declare a VOLUME for the journal directory — the journal is the " +
      "only copy of the room's order, and a container layer is deleted with the container",
  );
  const directory = declared[1];

  for (const [service, command] of relayServices()) {
    const journal = command.find((argument) => argument.includes("/"));
    assert.ok(journal, `compose service ${service} passes no journal path`);
    assert.ok(
      journal.startsWith(`${directory}/`),
      `compose service ${service} keeps its journal at ${journal}, outside the declared ` +
        `volume ${directory} — it would be written into the container layer and lost`,
    );
  }

  const services = compose().services;
  for (const [name, service] of Object.entries(services)) {
    if (service.build?.dockerfile !== "Dockerfile.relay") continue;
    assert.ok(
      (service.volumes ?? []).some((mount) => mount.endsWith(`:${directory}`)),
      `compose service ${name} runs the relay without mounting a volume at ${directory}`,
    );
  }
});

test("the relay image ships no CMD, so it cannot default a permission nobody chose", () => {
  // The invariant, not a style rule. `serve` refuses to start without a role
  // because "a default would be a permission nobody chose"; a `CMD` in the image
  // would reintroduce exactly that default, one layer further out where the
  // binary's own refusal cannot see it.
  assert.ok(
    /a default would be a permission nobody chose/.test(MAIN_RS),
    "server/src/main.rs no longer states the no-default-role rule this guard protects — " +
      "if the binary gained a default role, this assertion is the thing to revisit, " +
      "deliberately",
  );
  assert.equal(
    RELAY_DOCKERFILE.match(/^CMD\s/m),
    null,
    "Dockerfile.relay declares a CMD. `serve` refuses to start without a role on purpose; " +
      "a CMD makes the image choose the room's permission ceiling for the operator. Pass " +
      "the whole command, role included, from docker-compose.yml",
  );
  assert.ok(
    /^ENTRYPOINT \["opendoc-relay"\]/m.test(RELAY_DOCKERFILE),
    "Dockerfile.relay must keep the binary as its ENTRYPOINT, so `docker run` prints the " +
      "usage line and exits 2 exactly as the binary does",
  );
});

test("the stage names docs/162 lists are the stages the editor image has", () => {
  // A small number, typed into prose, about a file in the same commit — which is
  // exactly the shape that goes stale. The first draft of §3.2 said "four
  // stages" over a three-stage Dockerfile, and nothing but a reader would have
  // noticed.
  const stages = [...EDITOR_DOCKERFILE.matchAll(/^FROM\s+\S+\s+AS\s+(\S+)/gm)].map(
    (match) => match[1],
  );
  assert.ok(stages.length >= 2, "Dockerfile.editor must be a multi-stage build");
  const counted = { 1: "one", 2: "two", 3: "three", 4: "four", 5: "five" }[stages.length];
  assert.match(
    GUIDE,
    new RegExp(`The build is ${counted} stages`),
    `docs/162 §3.2 must say the editor image is ${counted} stages — it has ` +
      `${stages.length}: ${stages.join(", ")}`,
  );
  for (const stage of stages) {
    assert.ok(
      GUIDE.includes(`\`${stage}\``),
      `docs/162 §3.2 names the stages and does not name \`${stage}\``,
    );
  }
  // The claim the stage split exists to make: nothing from the toolchain is in
  // the final stage. Read as "the last FROM onwards", which is the runtime.
  const runtime = EDITOR_DOCKERFILE.slice(EDITOR_DOCKERFILE.lastIndexOf("\nFROM "));
  assert.doesNotMatch(
    runtime,
    /\b(cargo|rustup|wasm-pack|apt-get|npm)\b/,
    "Dockerfile.editor's runtime stage reaches for the build toolchain, so docs/162 §3.2's " +
      '"no Rust, no cargo, no Node, no Python and no source" is false',
  );
});

test("neither runtime stage runs as root", () => {
  for (const [name, text] of [
    ["Dockerfile.relay", RELAY_DOCKERFILE],
    ["Dockerfile.editor", EDITOR_DOCKERFILE],
  ]) {
    const users = [...text.matchAll(/^USER\s+(\S+)/gm)].map((m) => m[1]);
    assert.ok(users.length > 0, `${name} must set a non-root USER`);
    const last = users[users.length - 1];
    assert.ok(
      !/^(root|0)(:|$)/.test(last),
      `${name}'s final USER is ${last}`,
    );
  }
});

test("the static server does not rewrite the self-hosted fonts away", () => {
  // `tests/chrome_fonts.test.mjs` asserts `src/fonts.css` carries no Google Fonts
  // host, which is what keeps the chrome working offline. That guarantee is
  // undone by anything in the serving layer that rewrites a URL, and nothing
  // else in the suite looks at the serving layer.
  assert.doesNotMatch(
    NGINX,
    /sub_filter/,
    "deploy/editor-nginx.conf uses sub_filter. The editor's fonts are self-hosted and " +
      "guarded; rewriting response bodies is how that guarantee gets undone in the one " +
      "place no other test is looking",
  );
  assert.doesNotMatch(
    NGINX,
    /fonts\.(?:googleapis|gstatic)\.com/,
    "deploy/editor-nginx.conf names a Google Fonts host",
  );
  // The GUARANTEE — a `.wasm` request is answered as `application/wasm` — and not
  // the first mechanism that was reached for. A `types { application/wasm wasm; }`
  // block at `http` level reads like the obvious way to say this and is
  // `nginx: [emerg] duplicate extension "wasm"`, because `include mime.types` has
  // mapped it since nginx 1.21 and a second mapping is a conflict rather than an
  // override. So what is asserted is a per-location pin: the extension, an emptied
  // types hash, and the type.
  const wasmLocation = NGINX.match(/location[^{]*\\\.wasm\$[^{]*\{([\s\S]*?)\n {8}\}/);
  assert.ok(
    wasmLocation,
    "deploy/editor-nginx.conf must carry a location for .wasm — a module served as " +
      "application/octet-stream is refused by WebAssembly.instantiateStreaming and the " +
      "editor loads blank",
  );
  assert.match(
    wasmLocation[1],
    /default_type application\/wasm;/,
    "the .wasm location must set default_type application/wasm",
  );
  assert.match(
    wasmLocation[1],
    /types \{ \}/,
    "the .wasm location must empty the types hash first, or default_type never applies " +
      "to a file mime.types already has a type for",
  );
  assert.doesNotMatch(
    NGINX,
    /^\s*types \{\s*$/m,
    "a multi-line `types {` block at http level will collide with include mime.types and " +
      "refuse to start: nginx: [emerg] duplicate extension",
  );
});

test("the pinned toolchain versions are the ones their sources name", () => {
  const channel = TOOLCHAIN.match(/channel\s*=\s*"([^"]+)"/);
  assert.ok(channel, "rust-toolchain.toml must pin a channel");
  for (const [name, text] of [
    ["Dockerfile.relay", RELAY_DOCKERFILE],
    ["Dockerfile.editor", EDITOR_DOCKERFILE],
  ]) {
    const base = text.match(/^FROM rust:([^\s-]+)-/m);
    assert.ok(base, `${name} must build on a pinned rust: image`);
    assert.equal(
      base[1],
      channel[1],
      `${name} builds on Rust ${base[1]} while rust-toolchain.toml pins ${channel[1]} — ` +
        "`cargo +1.96.0 fmt` is the required formatter precisely because the versions " +
        "behave differently",
    );
    const override = text.match(/RUSTUP_TOOLCHAIN=(\S+)/);
    assert.ok(override, `${name} must pin RUSTUP_TOOLCHAIN`);
    assert.equal(override[1], channel[1], `${name}'s RUSTUP_TOOLCHAIN is not the pinned channel`);
  }
  assert.equal(
    statedConstant("The Rust toolchain both images pin"),
    channel[1],
    "docs/162 §2 states a Rust version that is not the pinned channel",
  );

  const wasmPack = CI.match(/tool:\s*wasm-pack@(\S+)/);
  assert.ok(wasmPack, "ci.yml must install a pinned wasm-pack");
  const inImage = EDITOR_DOCKERFILE.match(/ARG WASM_PACK_VERSION=(\S+)/);
  assert.ok(inImage, "Dockerfile.editor must pin WASM_PACK_VERSION");
  assert.equal(
    inImage[1],
    wasmPack[1],
    "Dockerfile.editor installs a different wasm-pack from CI. wasm-pack decides which " +
      "wasm-bindgen and wasm-opt are fetched, so a drift here is a different module than " +
      "the one CI tested",
  );
  assert.equal(
    statedConstant("The wasm-pack version the editor image installs"),
    wasmPack[1],
    "docs/162 §2 states a wasm-pack version the image does not install",
  );

  const node = CI.match(/node-version:\s*"(\d+)"/);
  assert.ok(node, "ci.yml must pin a Node major");
  assert.match(
    EDITOR_DOCKERFILE,
    new RegExp(`FROM node:${node[1]}-`),
    `Dockerfile.editor must build with the Node major CI uses (${node[1]}) — build.sh runs ` +
      "five generators whose --check failing for a runtime reason is indistinguishable " +
      "from real drift",
  );
  assert.equal(
    statedConstant("The Node major the editor image builds with"),
    node[1],
    "docs/162 §2 states a Node major the image does not use",
  );
});

test("the participant ceiling docs/162 publishes is the constant, not a number typed there", () => {
  const constant = PRESENCE_RS.match(/pub const MAX_PARTICIPANTS:\s*usize\s*=\s*(\d+)/);
  assert.ok(constant, "casual-doc-transaction/src/presence.rs must define MAX_PARTICIPANTS");
  assert.equal(
    statedConstant("Participants one room will hold"),
    constant[1],
    "docs/162 §2 publishes a participant ceiling that is not MAX_PARTICIPANTS. §11 sizes " +
      "the relay on this number, so a stale copy here is a stale capacity claim",
  );
  assert.match(
    GUIDE,
    new RegExp(`\\b${constant[1]}-participant ceiling\\b`),
    "docs/162 must describe the ceiling in §5.4/§8/§11 using the real constant",
  );
});

test("the refusal docs/162 names for a full room is the code the relay sends", () => {
  const code = statedConstant("The refusal a full room answers with");
  const tests = read("server", "src", "relay_tests.rs");
  assert.match(
    tests,
    new RegExp(`Refusal::RoomFull\\s*{[^}]*}\\.code\\(\\),\\s*"${code}"`),
    `docs/162 publishes ${code} as the full-room refusal, and server/src/relay_tests.rs ` +
      "does not pin that code for Refusal::RoomFull. One of the two is wrong, and the " +
      "guide is the one a reader believes",
  );
  assert.match(
    GUIDE,
    /collaboration_room_full/,
    "docs/162 should name the refusal's wire name beside its code",
  );
});

test("docs/162 claims no CI job builds these images, because none does", () => {
  // `SKILL.md` §9.2: prose describing a CI gate must name the workflow, and a
  // test must assert the gate is armed. The guide claims the opposite — that
  // there is no such job — so what has to be asserted is that the claim is still
  // true, and this assertion is what turns adding one into a documentation
  // change rather than a silent upgrade of a published promise.
  const builds = /docker\s+(?:build|compose)/.test(CI);
  assert.equal(
    builds,
    false,
    "ci.yml now runs a docker build, so docs/162 §8.7's \"No CI job builds these images\" " +
      "is false. Update the guide and name the workflow and the job, per SKILL §9.2",
  );
  assert.match(
    GUIDE,
    /No CI job builds these images/,
    "docs/162 must keep saying that no CI job builds these images, for as long as that is " +
      "what is true",
  );
});

test("the relay stays optional in the compose file, not merely in the prose", () => {
  const services = compose().services;
  for (const [name, service] of Object.entries(services)) {
    if (service.build?.dockerfile !== "Dockerfile.relay") continue;
    assert.ok(
      Array.isArray(service.profiles) && service.profiles.length > 0,
      `compose service ${name} runs the relay with no profile, so \`docker compose up\` ` +
        "would start it. No mandatory server is a structural property of this project " +
        "(server/src/lib.rs), and a compose file that starts a relay by default teaches " +
        "the opposite of the architecture",
    );
  }
  assert.ok(
    !Array.isArray(services.editor.profiles),
    "the editor service must NOT be behind a profile — it is the product",
  );
});

// ===========================================================================
// ONE CONTAINER BY DEFAULT, and no service that is really a command.
//
// The owner's question was "why are there 4 container" — and the compose file
// did declare four services against two images, two of which (`relay-create`,
// `relay-inspect`) were one-shot administrative commands dressed as services:
// they run, print, and exit. A service that exits immediately is noise in
// `docker compose ps` and an invitation to `up` it, and four entries teach a
// deployer that this is a four-part stack when `docker compose up` has always
// started exactly one container.
//
// The three guards below are what makes removing them a removal of CLUTTER
// rather than of COVERAGE: the file may not carry a service that exits, the
// default `up` set is pinned at one, and every subcommand no service runs must
// be published as a runnable invocation in `docs/162` — where it is then held
// against `server/src/main.rs` exactly as a compose `command:` is.

/** The subcommand that binds a port, read out of the binary's own bindings: the
 *  one whose operand list names an address is the long-running one, and the
 *  others run and exit. Derived rather than named, so a renamed `serve` does not
 *  silently turn these guards off. */
function longRunningSubcommand() {
  const commands = binaryInterface();
  const serving = [...commands].filter(([, operands]) => operands.includes("address"));
  assert.equal(
    serving.length,
    1,
    "exactly one of server/src/main.rs's subcommands should bind an address — the others " +
      `run and exit. Found: ${serving.map(([name]) => name).join(", ") || "none"}`,
  );
  return serving[0][0];
}

test("no compose service is a one-shot command dressed as a service", () => {
  const serve = longRunningSubcommand();
  const services = compose().services;
  const offenders = [];
  for (const [name, service] of Object.entries(services)) {
    if (Array.isArray(service.command) && service.command[0] !== serve) {
      offenders.push(`${name}: runs \`${service.command[0]}\`, which exits`);
    }
    // `restart: "no"` is the tell on a service whose author knew it would exit.
    if (service.restart === "no") offenders.push(`${name}: declares restart: "no"`);
  }
  assert.deepEqual(
    offenders,
    [],
    "a compose service must be a process that stays up. `create` and `inspect` run and " +
      "exit, so a service for each is two rows in `docker compose ps` that are never up. " +
      "They belong in `docker compose run --rm <relay service> <subcommand>`, which " +
      "overrides the command, mounts the same volume and publishes no port — and docs/162 " +
      "must then publish each one (see the invocation guard below)",
  );
});

test("`docker compose up` with no profile starts exactly one container", () => {
  const services = compose().services;
  const byDefault = Object.entries(services)
    .filter(([, service]) => !Array.isArray(service.profiles) || service.profiles.length === 0)
    .map(([name]) => name);
  assert.deepEqual(
    byDefault,
    ["editor"],
    "the default `docker compose up` must start the editor and nothing else. ONLYOFFICE " +
      "Document Server and Collabora CODE each ship a single container because the server " +
      "IS their product; this project's single container is one with NO server in it, which " +
      "is a stronger claim and only true while this list has one entry",
  );
  // And the counts the guide publishes, so a reader quoting §2 is quoting the file.
  assert.equal(
    Number(statedConstant("Services the compose file declares")),
    Object.keys(services).length,
    "docs/162 §2 states a number of compose services the file does not declare",
  );
  assert.equal(
    Number(statedConstant("Containers a default `docker compose up` starts")),
    byDefault.length,
    "docs/162 §2 states a default container count the compose file does not produce",
  );
});

/** Every line inside a ```sh fence of docs/162 — the runnable ones. The usage
 *  block in §5.1 is a ```text fence on purpose: it carries `<journal>`-style
 *  placeholders rather than a command, and reading it here would be reading
 *  prose as an invocation. */
function guideShellLines() {
  const lines = [];
  let fence = null;
  GUIDE.split("\n").forEach((line, index) => {
    const open = line.match(/^```(\w*)/);
    if (open) {
      fence = fence === null ? open[1] : null;
      return;
    }
    if (fence === "sh" && line.trim() && !line.trim().startsWith("#")) {
      lines.push({ at: index + 1, text: line.trim() });
    }
  });
  assert.ok(lines.length > 5, "docs/162 must carry runnable shell blocks");
  return lines;
}

/** The relay invocations docs/162 publishes, in the two shapes it uses:
 *
 *    docker compose run [flags] <service> <subcommand> [operands]
 *    opendoc-relay <subcommand> [operands]              (inside a docker run)
 *
 *  A line that overrides the entrypoint is NOT the first shape — `--entrypoint
 *  sh` runs a shell, and the relay invocation it carries (if any) is caught by
 *  the second shape instead. */
function publishedRelayInvocations() {
  const found = [];
  const flagTakesValue = new Set(["--entrypoint", "-v", "--volume", "-e", "--env", "--name"]);
  for (const { at, text } of guideShellLines()) {
    const run = text.match(/^docker compose run\s+(.*)$/);
    if (run && !/--entrypoint\b/.test(text)) {
      const tokens = run[1].split(/\s+/);
      while (tokens.length && tokens[0].startsWith("-")) {
        const flag = tokens.shift();
        if (flagTakesValue.has(flag)) tokens.shift();
      }
      const [service, subcommand, ...operands] = tokens;
      found.push({ at, how: `docker compose run … ${service}`, service, subcommand, operands });
    }
    for (const direct of text.matchAll(/opendoc-relay\s+([a-z]+)((?:\s+[^\s'"&|]+)*)/g)) {
      found.push({
        at,
        how: "opendoc-relay",
        service: null,
        subcommand: direct[1],
        operands: direct[2].trim() ? direct[2].trim().split(/\s+/) : [],
      });
    }
  }
  return found;
}

test("every relay invocation docs/162 publishes is one the binary matches", () => {
  const commands = binaryInterface();
  const relays = relayServices();
  const volume = RELAY_DOCKERFILE.match(/^VOLUME\s+\["([^"]+)"\]/m)[1];
  const shape = (operand) => {
    if (/^\d+\.\d+\.\d+\.\d+:\d+$|^\[?[0-9a-f:]*\]?:\d+$/i.test(operand)) return "address";
    if (operand.includes("/")) return "journal";
    return "role";
  };

  const invocations = publishedRelayInvocations();
  assert.ok(
    invocations.length > 0,
    "docs/162 publishes no relay invocation at all, so either the guide stopped documenting " +
      "the administrative commands or this reader stopped finding them — and in both cases " +
      "the coverage the one-shot compose services used to carry is gone",
  );

  for (const { at, how, service, subcommand, operands } of invocations) {
    if (service !== null) {
      assert.ok(
        relays.has(service),
        `docs/162 line ${at} runs \`docker compose run … ${service}\`, which is not a ` +
          `service that builds Dockerfile.relay. It knows: ${[...relays.keys()].join(", ")}`,
      );
    }
    assert.ok(
      commands.has(subcommand),
      `docs/162 line ${at} (${how}) runs \`${subcommand}\`, which server/src/main.rs does ` +
        `not match. It matches: ${[...commands.keys()].join(", ")}`,
    );
    const expected = commands.get(subcommand);
    assert.equal(
      operands.length,
      expected.length,
      `docs/162 line ${at} passes ${operands.length} operand(s) to \`${subcommand}\`, which ` +
        `takes ${expected.length} (${expected.join(" ")}) — a wrong arity exits 2 with the ` +
        "usage line, and this one is published for an operator to paste",
    );
    operands.forEach((operand, index) => {
      assert.equal(
        shape(operand),
        expected[index],
        `docs/162 line ${at}: \`${subcommand}\` argument ${index + 1} is ` +
          `${shape(operand)}-shaped (${operand}), but main.rs binds it as ` +
          `\`${expected[index]}\` — the positions are the whole interface`,
      );
      if (expected[index] === "journal") {
        assert.ok(
          operand.startsWith(`${volume}/`),
          `docs/162 line ${at} names a journal at ${operand}, outside the volume ` +
            `${volume} that Dockerfile.relay declares — written there it lives in the ` +
            "container layer and is deleted with the container",
        );
      }
    });
  }

  // THE COVERAGE ASSERTION, and the reason this test is not optional. Every
  // subcommand the binary has that no compose service runs is now reachable only
  // through the guide, so the guide must publish a runnable invocation of it.
  // Derived from both sides: delete a `create` example and this fails; add a
  // fourth subcommand to the binary and this fails until it is documented.
  const inCompose = new Set([...relays.values()].map((command) => command[0]));
  const published = new Set(invocations.map((invocation) => invocation.subcommand));
  const undocumented = [...commands.keys()].filter(
    (name) => !inCompose.has(name) && !published.has(name),
  );
  assert.deepEqual(
    undocumented,
    [],
    "these subcommands are run by no compose service and published by no runnable example " +
      "in docs/162, so nothing tells an operator how to run them and nothing checks the " +
      "invocation when it changes. That is what deleting the one-shot services would have " +
      "cost if the guide did not carry them",
  );
});

test("the embedding guide's deployment commands are derived from the binary too", () => {
  // `webapp/tools/build-embed-docs.mjs` generates the page's command panel, and
  // it now reads `server/src/main.rs` for the administrative subcommand names
  // rather than carrying them as literals — which is what stops the published
  // page naming a `relay-create` service, or a subcommand, that does not exist.
  // Asserted here because the generator's own failure mode is silence: it would
  // happily print whatever string it was given.
  const generator = readFileSync(join(WEBAPP, "tools", "build-embed-docs.mjs"), "utf8");
  assert.match(
    generator,
    /join\(REPO,\s*"server",\s*"src",\s*"main\.rs"\)/,
    "build-embed-docs.mjs must read server/src/main.rs for the relay's subcommands, so the " +
      "deployment panel cannot publish a command the binary does not match",
  );
  const page = readFileSync(join(WEBAPP, "embedding.page.html"), "utf8");
  const panel = page.slice(
    page.indexOf("@generated container-commands"),
    page.indexOf("@end container-commands"),
  );
  assert.ok(panel, "embedding.page.html must carry the generated container-commands region");
  assert.doesNotMatch(
    panel,
    /relay-(create|inspect)/,
    "the embedding page still names a one-shot relay service. The compose file has two " +
      "services; regenerate the page (node webapp/tools/build-embed-docs.mjs)",
  );
  // Each subcommand is reachable from the panel in the one shape that suits it:
  // the long-running one by bringing its profile up, the one-shots by
  // `docker compose run` against the same service.
  const serve = longRunningSubcommand();
  const [relay, command] = [...relayServices()][0];
  for (const [subcommand] of binaryInterface()) {
    const expected =
      subcommand === serve
        ? `--profile ${compose().services[relay].profiles[0]} up`
        : `run --rm ${relay} ${subcommand} ${command.find((argument) => argument.includes("/"))}`;
    assert.ok(
      panel.includes(expected),
      `the deployment panel does not publish \`${expected}\`, so \`${subcommand}\` is not ` +
        "reachable from the page that tells a host how to deploy this",
    );
  }
});

// Every third-party action is pinned to a commit, not to a tag.
//
// A tag is mutable: whoever owns the action can move `v4` to different code
// after review, and `peter-evans/create-pull-request` in `oracle-geometry.yml`
// can open pull requests against this repository. A commit SHA cannot be moved.
//
// This guard exists because the repository was already 40-of-47 pinned and
// nobody could see the remaining seven. Five of them were in `pages.yml` and
// one in `oracle-geometry.yml`, left behind when the rest were pinned, and the
// only thing pointing at them was a stack of dependabot pull requests proposing
// tag-to-tag bumps — which is a different change, and one that cannot fix a
// mutable reference. Counting is how it stays visible: an inconsistency spread
// spread over two of the four workflow files is invisible to review and obvious
// only to a grep that nobody runs.
//
// Deliberately NOT asserted: that the pinned version is the newest available.
// That is dependabot's job, needs the network, and would make this guard go red
// on its own one day with no change to this repository.
test("every workflow action is pinned to a commit SHA, never to a tag", () => {
  const dir = join(REPO, ".github", "workflows");
  const files = readdirSync(dir).filter((name) => name.endsWith(".yml"));
  assert.ok(
    files.length >= 4,
    `expected to read this repository's workflows, found ${files.length} — a guard ` +
      "that reads no files passes for the wrong reason",
  );

  const unpinned = [];
  let total = 0;
  for (const name of files) {
    readFileSync(join(dir, name), "utf8")
      .split("\n")
      .forEach((line, index) => {
        const used = line.match(/^\s*(?:-\s*)?uses:\s*(\S+)/);
        if (!used) return;
        const reference = used[1];
        // A local action (`./.github/actions/x`) has no version to pin.
        if (reference.startsWith("./")) return;
        total += 1;
        const [, after] = reference.split("@");
        if (!/^[0-9a-f]{40}$/.test(after ?? "")) {
          unpinned.push(`${name}:${index + 1}: ${reference}`);
        }
      });
  }

  assert.ok(
    total >= 40,
    `expected to find this repository's action uses, found ${total} — the \`uses:\` ` +
      "shape changed and this guard is reading nothing",
  );
  assert.deepEqual(
    unpinned,
    [],
    "these workflow actions are pinned to a mutable tag; pin each to a 40-character " +
      "commit SHA with the version in a trailing comment",
  );
});
