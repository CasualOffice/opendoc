// Reads one named part out of an OPC package (a .docx/.odt a spec just saved),
// shared by the specs that check what a save wrote.
import { inflateRawSync } from "node:zlib";

/** One named part out of an OPC package, as text.
 *
 *  Twenty lines of local-file-header walk rather than a dependency, and rather
 *  than a raw byte search for the string: an OPC writer may store a part
 *  deflated, in which case `bytes.includes("…template…")` silently fails to find
 *  a content type that IS there — a guard that passes or fails on the
 *  compression setting rather than on what the file says. Handles both stored
 *  (method 0) and deflated (method 8) entries, which is every entry these
 *  writers produce.
 *
 *  Complexity: O(bytes) once, over a document the test itself just exported. */
export function opcPart(zip, name) {
  let at = 0;
  while (at + 30 <= zip.length && zip.readUInt32LE(at) === 0x0403_4b50) {
    const method = zip.readUInt16LE(at + 8);
    const compressed = zip.readUInt32LE(at + 18);
    const nameLength = zip.readUInt16LE(at + 26);
    const extraLength = zip.readUInt16LE(at + 28);
    const entry = zip.subarray(at + 30, at + 30 + nameLength).toString("utf8");
    const dataAt = at + 30 + nameLength + extraLength;
    const data = zip.subarray(dataAt, dataAt + compressed);
    if (entry === name) {
      return (method === 8 ? inflateRawSync(data) : data).toString("utf8");
    }
    at = dataAt + compressed;
  }
  return null;
}
