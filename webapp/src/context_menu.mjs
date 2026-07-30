const VIEWPORT_INSET = 8;

export function clampContextMenuPosition(
  x,
  y,
  width,
  height,
  viewportWidth,
  viewportHeight,
  inset = VIEWPORT_INSET,
) {
  return {
    left: Math.max(inset, Math.min(x, viewportWidth - width - inset)),
    top: Math.max(inset, Math.min(y, viewportHeight - height - inset)),
  };
}

export function normalizeMenuEntries(commands) {
  const entries = [];
  let previousGroup = null;
  for (const command of commands.filter((candidate) => candidate.visible !== false)) {
    if (previousGroup !== null && command.group !== previousGroup) {
      entries.push({ separator: true });
    }
    entries.push(command);
    previousGroup = command.group;
  }
  return entries;
}

export function enabledMenuIndexes(entries) {
  return entries.flatMap((entry, index) =>
    !entry.separator && entry.enabled !== false ? [index] : []);
}

export function moveMenuIndex(entries, current, direction) {
  const enabled = enabledMenuIndexes(entries);
  if (!enabled.length) return -1;
  const position = enabled.indexOf(current);
  if (direction === "first") return enabled[0];
  if (direction === "last") return enabled.at(-1);
  if (position < 0) return direction === -1 ? enabled.at(-1) : enabled[0];
  return enabled[(position + direction + enabled.length) % enabled.length];
}
