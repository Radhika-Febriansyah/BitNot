import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";

interface DialogProps {
  open: boolean;
  title: string;
  onClose: () => void;
  wide?: boolean;
  children: ReactNode;
}

/** Modal built on the native <dialog>: focus is trapped and Esc closes it. */
export function Dialog({ open, title, onClose, wide, children }: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    if (open && !node.open) node.showModal();
    if (!open && node.open) node.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      className={wide ? "dialog wide" : "dialog"}
      aria-labelledby={titleId}
      onClose={onClose}
      onClick={(event) => {
        if (event.target === ref.current) onClose(); // click on the backdrop
      }}
    >
      {open && (
        <>
          <h2 id={titleId}>{title}</h2>
          {children}
        </>
      )}
    </dialog>
  );
}

interface ConfirmProps {
  open: boolean;
  title: string;
  message: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

export function ConfirmDialog(props: ConfirmProps) {
  return (
    <Dialog open={props.open} title={props.title} onClose={props.onClose}>
      <div className="muted">{props.message}</div>
      <div className="dialog-actions">
        <button className="btn btn-ghost" onClick={props.onClose} autoFocus>
          Cancel
        </button>
        <button
          className={props.danger ? "btn btn-danger" : "btn btn-primary"}
          onClick={() => {
            props.onConfirm();
            props.onClose();
          }}
        >
          {props.confirmLabel}
        </button>
      </div>
    </Dialog>
  );
}

interface PromptProps {
  open: boolean;
  title: string;
  label: string;
  initialValue?: string;
  submitLabel: string;
  maxLength?: number;
  onSubmit: (value: string) => void;
  onClose: () => void;
}

export function PromptDialog(props: PromptProps) {
  return (
    <Dialog open={props.open} title={props.title} onClose={props.onClose}>
      {props.open && <PromptForm {...props} />}
    </Dialog>
  );
}

function PromptForm(props: PromptProps) {
  const [value, setValue] = useState(props.initialValue ?? "");
  const inputId = useId();

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!value.trim()) return;
    props.onSubmit(value.trim());
    props.onClose();
  };

  return (
    <form onSubmit={submit}>
      <label htmlFor={inputId} className="field-label">
        {props.label}
      </label>
      <input
        id={inputId}
        className="input selectable"
        style={{ width: "100%" }}
        value={value}
        maxLength={props.maxLength}
        onChange={(event) => setValue(event.target.value)}
        autoFocus
      />
      <div className="dialog-actions">
        <button type="button" className="btn btn-ghost" onClick={props.onClose}>
          Cancel
        </button>
        <button type="submit" className="btn btn-primary" disabled={!value.trim()}>
          {props.submitLabel}
        </button>
      </div>
    </form>
  );
}
