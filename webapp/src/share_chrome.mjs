import { registerModal } from './modal.mjs';

/** Share, at the right end of the header's document row — after the state chips and
 *  the people in the room, where Google Docs, Word on the web and ONLYOFFICE all put
 *  it (`docs/109` HF-330). It was appended to the title row, beside the file name,
 *  which painted it in the middle of the header.
 *
 *  The header's existing primary button (`.btn-primary`), with a glyph so the phone
 *  rung can drop the word and keep the control (`style.css`, `.share-btn`). The name
 *  is on `aria-label` as well as in the text, because the text is the part the phone
 *  rung hides.
 *
 *  Uses the editor's modal contract so application shortcuts cannot fire behind the link. */
export function createShareChrome({ root, parent, share, available, t }) {
  const button = root.createElement('button');
  button.id = 'shareBtn'; button.type = 'button'; button.className = 'btn btn-primary share-btn';
  const icon = root.createElement('span'); icon.className = 'ms'; icon.setAttribute('aria-hidden', 'true'); icon.textContent = 'person_add';
  const text = root.createElement('span'); text.className = 'share-label';
  button.append(icon, text);
  parent.append(button);
  let dialog, controller;
  return {
    button,
    reflect() {
      text.textContent = t('collab.share');
      button.setAttribute('aria-label', t('collab.share'));
      button.disabled = !available();
      button.title = t(button.disabled ? 'collab.shareHostRequired' : 'collab.share');
    },
    bind() { button.addEventListener('click', () => share()); },
    showLink(link) {
      if (!dialog) {
        dialog = root.createElement('div'); dialog.className = 'dialog-overlay'; dialog.hidden = true;
        dialog.id = 'sharedLinkDialog'; dialog.setAttribute('role','dialog'); dialog.setAttribute('aria-modal','true');
        const panel = root.createElement('div'); panel.className = 'dialog shared-link-dialog';
        const heading = root.createElement('h2'); heading.id = 'sharedLinkHeading'; heading.textContent = t('collab.share');
        dialog.setAttribute('aria-labelledby',heading.id);
        const label = root.createElement('label'); label.textContent = t('collab.shareLink');
        const input = root.createElement('input'); input.readOnly = true; input.type = 'url'; label.append(input);
        const close = root.createElement('button'); close.type = 'button'; close.textContent = t('collab.close');
        close.addEventListener('click', () => controller.close());
        panel.append(heading,label,close); dialog.append(panel); root.body.append(dialog);
        controller = registerModal(dialog,{initialFocus:() => input,fallbackFocus:() => button,onOpen:() => input.select()});
      }
      dialog.querySelector('input').value = link;
      controller.open();
    },
  };
}
