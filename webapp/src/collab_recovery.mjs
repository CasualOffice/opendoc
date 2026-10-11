// A host supplies a checked snapshot, while the engine retains and replays work.
export function createRecovery({ document, transport, provider, room, recovered, status, t }) {
  let active = null, needed = false, generation = 0, minimum = 0;
  return {
    receive(outcome) {
      if (outcome.kind !== "recoveryRequired") return active;
      minimum = Math.max(minimum, outcome.revision);
      if (active) return active;
      needed = true;
      const doc = document(), host = provider();
      const epoch = generation;
      status(t("collab.recoveryNeeded"), "warn");
      if (!doc || typeof host?.snapshotFor !== "function") return null;
      active = (async () => {
        try {
          const snapshot = await host.snapshotFor({ room: room(), minimumRevision: minimum });
          if (document() !== doc || epoch !== generation) return;
          if (!(snapshot?.bytes instanceof Uint8Array) || snapshot.bytes.length > 64 * 1024 * 1024 ||
              !Number.isSafeInteger(snapshot.revision) || snapshot.revision < minimum) {
            throw new Error(t("collab.snapshotInvalid"));
          }
          const result = JSON.parse(doc.collabRecover(snapshot.bytes, snapshot.revision));
          await recovered(result);
          if (document() !== doc || epoch !== generation) return;
          needed = false;
          transport()?.resync();
          status(t(result.retained ? "collab.recoveryRetained" : "collab.recoveryComplete",
            { replayed: result.replayed, retained: result.retained }), result.retained ? "warn" : "");
        } catch (error) {
          status(`${t("collab.recoveryNeeded")} ${String(error.message || error)}`, "warn");
        } finally { if (epoch === generation) active = null; }
      })();
      return active;
    },
    pending: () => active !== null,
    needed: () => needed,
    reset() { generation++; active = null; needed = false; minimum = 0; },
  };
}
