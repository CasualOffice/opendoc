// Word's Change Picture, host half (`picture_replace.mjs`, `docs/109` HF-252).
import assert from "node:assert/strict";
import test from "node:test";

const { EMU_PER_PX, INSERTABLE_IMAGE_TYPES, canChangePicture, createPictureReplace } = await import(
  "../src/picture_replace.mjs"
);

const picture = { kind: "image", mode: "selected", node: "n1" };

/** A recording host. `runEditResult` is what the gated edit path answers. */
function host({ blocked = false, runEditResult = true, alt = null, decode } = {}) {
  const log = { status: [], edits: [], inputs: [] };
  const doc = {
    objectDescr: () => alt,
    replacePicture: (...args) => {
      log.edits.push(args);
      return { ok: true };
    },
  };
  const io = {
    doc: () => doc,
    selection: () => picture,
    runEdit: async (thunk) => {
      if (runEditResult) thunk();
      return runEditResult;
    },
    blocked: () => blocked,
    decode: decode ?? (async () => ({ bytes: new Uint8Array([1]), widthPx: 40, heightPx: 20, mime: "image/png" })),
    setStatus: (text, kind) => log.status.push([text, kind ?? ""]),
    t: (key) => key,
    createInput: () => {
      const listeners = {};
      const input = {
        files: null,
        addEventListener: (name, fn) => (listeners[name] = fn),
        click: () => log.inputs.push(input),
        fire: (file) => {
          input.files = [file];
          listeners.change();
        },
      };
      return input;
    },
  };
  return { io, log };
}

const file = (type = "image/png") => ({ type });

test("only a picture can have its image changed", () => {
  assert.equal(canChangePicture(picture), true);
  for (const kind of ["chart", "shape", "textbox", "group", "diagram"]) {
    assert.equal(canChangePicture({ ...picture, kind }), false, kind);
  }
  assert.equal(canChangePicture({ ...picture, mode: "editing" }), false);
  assert.equal(canChangePicture(null), false);
});

test("the picker offers exactly the types the engine can place", () => {
  const { io, log } = host();
  createPictureReplace(io).choose();
  assert.equal(log.inputs.length, 1);
  assert.equal(log.inputs[0].type, "file");
  assert.deepEqual(log.inputs[0].accept.split(","), [...INSERTABLE_IMAGE_TYPES]);
});

test("a refused review mode refuses BEFORE the picker opens", () => {
  const { io, log } = host({ blocked: true });
  createPictureReplace(io).choose();
  assert.equal(log.inputs.length, 0, "nobody chooses a file only to be told nothing could happen");
});

test("the chosen file reaches the engine at its natural size, for the picture it was chosen for", async () => {
  const { io, log } = host();
  const replace = createPictureReplace(io);
  await replace.replaceWith(file(), "n1");
  assert.deepEqual(log.edits[0].slice(0, 1), ["n1"]);
  assert.equal(log.edits[0][2], 40 * EMU_PER_PX);
  assert.equal(log.edits[0][3], 20 * EMU_PER_PX);
  assert.equal(log.edits[0][4], "image/png");
  assert.deepEqual(log.status, [["object.changePicture.done", ""]]);
});

test("with alt text on the picture, the status asks for it to be checked", async () => {
  const { io, log } = host({ alt: "Our logo" });
  await createPictureReplace(io).replaceWith(file(), "n1");
  assert.deepEqual(log.status, [["object.changePicture.doneAltText", ""]]);
});

test("an unsupported or unreadable file says why and changes nothing", async () => {
  const unsupported = host();
  await createPictureReplace(unsupported.io).replaceWith(file("application/pdf"), "n1");
  assert.deepEqual(unsupported.log.edits, []);
  assert.deepEqual(unsupported.log.status, [["object.changePicture.unsupported", "error"]]);

  const unreadable = host({ decode: async () => Promise.reject(new Error("bad bitmap")) });
  await createPictureReplace(unreadable.io).replaceWith(file(), "n1");
  assert.deepEqual(unreadable.log.edits, []);
  assert.deepEqual(unreadable.log.status, [["object.changePicture.unreadable", "error"]]);
});

test("an edit the gate refused does not claim success", async () => {
  const { io, log } = host({ runEditResult: false });
  await createPictureReplace(io).replaceWith(file(), "n1");
  assert.deepEqual(log.status, [], "the gate already said why; a success line here would contradict it");
});
