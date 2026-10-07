import { type Component, type JSX, Show, createEffect, createUniqueId } from 'solid-js';
import { createFocusTrap } from '../focus-trap';

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  id?: string;
  children: JSX.Element;
}

const Modal: Component<ModalProps> = (props) => {
  let modalEl: HTMLDivElement | undefined;
  const titleId = props.id ?? createUniqueId();
  const trap = createFocusTrap(() => modalEl, props.onClose);

  createEffect(() => {
    if (props.open) {
      trap.activate();
    } else {
      trap.deactivate();
    }
  });

  return (
    <Show when={props.open}>
      <div class="modal-overlay" onClick={props.onClose}>
        <div
          class="modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          ref={modalEl}
          onClick={(e) => e.stopPropagation()}
        >
          <div class="modal-header">
            <h2 id={titleId}>{props.title}</h2>
            <button
              class="modal-close"
              aria-label="Close"
              onClick={props.onClose}
            >&times;</button>
          </div>
          {props.children}
        </div>
      </div>
    </Show>
  );
};

export default Modal;
