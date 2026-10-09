// Reads one named part out of an OPC package (a .docx/.odt a spec just saved),
// shared by the specs that check what a save wrote.
import { inflateRawSync } from "node:zlib";

/** One named part out of an OPC package, as text, or null when there is none.
 *
 *  A short ZIP reader rather than a dependency, and rather than a raw byte
 *  search for the string: an OPC writer may store a part deflated, in which case
 *  `bytes.includes("…template…")` silently fails to find a content type that IS
 *  there — a guard that passes or fails on the compression setting rather than
 *  on what the file says.
 *
 *  It reads the CENTRAL DIRECTORY, not the local headers. A save that hands the
 *  source back unchanged returns the file as its producer wrote it, and Word,
 *  LibreOffice and this repository's own `demo.docx` write entries with a data
 *  descriptor (general-purpose flag bit 3), whose local header carries ZERO
 *  sizes; only the central directory says how long each entry is. Handles stored
 *  (method 0) and deflated (method 8) entries.
 *
 *  Complexity: O(entries) to find the part, O(its bytes) to inflate it. */
export function opcPart(zip, name) {
  // The end-of-central-directory record: the last 22 bytes, plus a comment of up
  // to 64 KiB before it.
  let end = -1;
  for (let at = zip.length - 22; at >= Math.max(0, zip.length - 22 - 0xffff); at -= 1) {
    if (zip.readUInt32LE(at) === 0x0605_4b50) {
      end = at;
      break;
    }
  }
  if (end < 0) return null;
  const entries = zip.readUInt16LE(end + 10);
  let at = zip.readUInt32LE(end + 16);
  for (let index = 0; index < entries && zip.readUInt32LE(at) === 0x0201_4b50; index += 1) {
    const method = zip.readUInt16LE(at + 10);
    const compressed = zip.readUInt32LE(at + 20);
    const nameLength = zip.readUInt16LE(at + 28);
    const extraLength = zip.readUInt16LE(at + 30);
    const commentLength = zip.readUInt16LE(at + 32);
    const localAt = zip.readUInt32LE(at + 42);
    const entry = zip.subarray(at + 46, at + 46 + nameLength).toString("utf8");
    if (entry === name) {
      const dataAt = localAt + 30 + zip.readUInt16LE(localAt + 26) + zip.readUInt16LE(localAt + 28);
      const data = zip.subarray(dataAt, dataAt + compressed);
      return (method === 8 ? inflateRawSync(data) : data).toString("utf8");
    }
    at += 46 + nameLength + extraLength + commentLength;
  }
  return null;
}
