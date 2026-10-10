/** Preserves unchanged imported geometries, including per-column spacing.
 * Authoring is bounded to the dialog's four editable columns. */
export function createColumnEditor({ el, measure, count, gap, separator, contentWidth }) {
  const equal = el('pageColumnEqual');
  const widths = Array.from({ length: 4 }, (_, i) => el(`pageColumnWidth${i}`));
  const spaces = Array.from({ length: 4 }, (_, i) => el(`pageColumnSpace${i}`));
  const fields = [...widths, ...spaces];
  const twips = input => measure.read(input.value);
  let painted = null;
  function enabled() {
    const n = Number(count.value);
    widths.forEach((input, i) => { input.disabled = equal.checked || i >= n; input.closest('label').hidden = i >= n; });
    spaces.forEach((input, i) => { input.disabled = equal.checked || i >= n - 1; input.closest('label').hidden = i >= n - 1; });
  }
  function seed() {
    const n = Number(count.value), spacing = twips(gap);
    const width = Math.max(1, Math.floor((contentWidth() - spacing * (n - 1)) / n));
    widths.forEach(input => { input.value = measure.format(width); });
    spaces.forEach(input => { input.value = measure.format(spacing); });
  }
  equal.addEventListener('change', () => { if (!equal.checked) seed(); enabled(); });
  count.addEventListener('change', () => { if (!equal.checked) seed(); enabled(); });
  function values() {
    const n = Number(count.value);
    return { count: n, spaceTwips: twips(gap), separator: separator.checked, equalWidth: equal.checked,
      columns: equal.checked ? [] : widths.slice(0, n).map((input, i) => ({ widthTwips: twips(input), ...(i < n - 1 ? { spaceTwips: twips(spaces[i]) } : {}) })) };
  }
  return {
    fields,
    reflect(columns) {
      equal.checked = columns?.equalWidth !== false;
      seed();
      for (const [i, column] of (columns?.columns ?? []).slice(0, 4).entries()) {
        widths[i].value = measure.format(column.widthTwips);
        spaces[i].value = measure.format(column.spaceTwips ?? columns.spaceTwips ?? 0);
      }
      enabled();
      painted = JSON.stringify(values());
    },
    payload(previous) {
      const value = values();
      if (JSON.stringify(value) === painted) return previous;
      return { ...(previous ?? {}), ...value };
    },
  };
}
