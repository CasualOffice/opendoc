/** Gallery clicks and palette actions share one mutation route. */
export function wireListGallery(menu, popover, { canApply, closePopover, apply }) {
  menu.addEventListener("click", (event) => {
    const cell = event.target.closest("[data-spec]");
    if (!cell || !canApply()) return;
    closePopover(popover);
    apply(cell.dataset.spec);
  });
}
