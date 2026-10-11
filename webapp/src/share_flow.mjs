import { roomUrl } from "./collab_transport.mjs";

/** Share documents are same-origin; an arbitrary link cannot fetch with ambient credentials. */
export function sharedDocument(base, raw) {
  if (typeof raw !== "string" || !raw) return null;
  try {
    const url = new URL(raw, base), origin = new URL(base);
    if (url.origin !== origin.origin || !["http:", "https:"].includes(url.protocol) || url.username || url.password) return null;
    return url.href;
  } catch { return null; }
}

/** A link contains the room, snapshot URL and exact snapshot revision, never a grant. */
export function shareLink(base, shared) {
  const room = roomUrl(shared?.room), documentUrl = sharedDocument(base, shared?.documentUrl);
  if (!room || !documentUrl || !Number.isSafeInteger(shared.revision) || shared.revision < 0) return null;
  const url = new URL("editor.html", base);
  if (url.protocol === "https:" && new URL(room).protocol !== "wss:") return null;
  url.searchParams.set("room", room);
  url.searchParams.set("document", documentUrl);
  url.searchParams.set("revision", String(shared.revision));
  return url.href;
}

export function createShare({ document, provider, base, name, joined, clipboard, showLink, status, t, existingLink = () => null }) {
  let busy = false;
  return {
    available: () => Boolean(document()) && (Boolean(existingLink()) || typeof provider()?.createShare === "function") && !busy,
    async share() {
      if (busy) return;
      const existing = existingLink();
      if (existing) {
        showLink(existing);
        try { await clipboard().writeText(existing); status(t("collab.shareCopied")); }
        catch { status(t("collab.shareCopyManually")); }
        return existing;
      }
      const doc = document(), host = provider();
      if (!doc || typeof host?.createShare !== "function") { status(t("collab.shareHostRequired"), "warn"); return; }
      busy = true;
      try {
        const epoch = doc.revision;
        const bytes = doc.collabSnapshot();
        const shared = await host.createShare({ bytes, name: name() });
        if (document() !== doc || doc.revision !== epoch) throw new Error(t("collab.shareDocumentChanged"));
        const link = shareLink(base(), shared);
        if (!link) throw new Error(t("collab.shareInvalid"));
        await joined(shared, link);
        showLink(link);
        try { await clipboard().writeText(link); status(t("collab.shareCopied")); }
        catch { status(t("collab.shareCopyManually")); }
        return link;
      } catch (error) { status(String(error.message || error), "error"); }
      finally { busy = false; }
    },
  };
}
