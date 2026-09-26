// The side-effect entry point: `import "@casualoffice/opendoc-embed/define"`
// and `<opendoc-editor>` works.
//
// Separate from the main entry so that one has no side effects and stays
// tree-shakeable — a host that wants the class, the capability tables or the
// role→mode mapping without registering a tag imports the package root.
import { defineOpenDocEditor } from "./embed_element.mjs";

defineOpenDocEditor();
