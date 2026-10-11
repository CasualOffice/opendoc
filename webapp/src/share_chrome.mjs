import { registerModal } from './modal.mjs';

/** Uses the editor's modal contract so application shortcuts cannot fire behind the link. */
export function createShareChrome({ root, parent, share, available, t }) {
  const button = root.createElement('button');
  button.id = 'shareBtn'; button.type = 'button'; button.className = 'btn';
  parent.append(button);
  let dialog, controller;
  return {
    reflect() {
      button.textContent = t('collab.share');
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
