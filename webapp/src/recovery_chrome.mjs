import { registerModal } from './modal.mjs';

/** Each retained document gets its own user download gesture; browsers may block bulk downloads. */
export function createRecoveryDownloader({ root, document, t }) {
  let dialog, list, controller;
  function save(doc, index) {
    if (document() !== doc) { controller?.close(); return; }
    const link = root.createElement('a');
    link.href = URL.createObjectURL(new Blob([doc.collabRecoveryDocument(index)],{type:'application/json'}));
    link.download = `opendoc-retained-edits-${index + 1}.json`; link.click();
    setTimeout(() => URL.revokeObjectURL(link.href),1000);
  }
  return () => {
    const doc = document(), count = doc?.collabRecoveryCount() || 0;
    if (count === 0) return;
    if (count === 1) { save(doc,0); return; }
    if (!dialog) {
      dialog = root.createElement('div'); dialog.className = 'dialog-overlay'; dialog.hidden = true;
      dialog.id = 'retainedDocumentsDialog'; dialog.setAttribute('role','dialog'); dialog.setAttribute('aria-modal','true');
      const panel = root.createElement('div'); panel.className = 'dialog shared-link-dialog';
      const heading = root.createElement('h2'); heading.id = 'retainedDocumentsHeading'; heading.textContent = t('collab.retainedDocuments');
      dialog.setAttribute('aria-labelledby',heading.id);
      list = root.createElement('div');
      const close = root.createElement('button'); close.type = 'button'; close.textContent = t('collab.close');
      close.addEventListener('click',() => controller.close());
      panel.append(heading,list,close); dialog.append(panel); root.body.append(dialog);
      controller = registerModal(dialog,{fallbackFocus:() => root.getElementById('pages')});
    }
    list.replaceChildren();
    for (let index = 0; index < count; index++) {
      const button = root.createElement('button'); button.type = 'button';
      button.textContent = t('collab.downloadDocument',{number:index + 1});
      button.addEventListener('click',() => save(doc,index)); list.append(button);
    }
    controller.open();
  };
}
