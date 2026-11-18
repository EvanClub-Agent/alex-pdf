import { useEffect, useId, useRef } from "react";
import clsx from "clsx";
import { X } from "lucide-react";

type ModalDialogProps = {
  open: boolean;
  onClose: () => void;
  title: string;
  subtitle?: string;
  large?: boolean;
  /** "any" allows backdrop light-dismiss + Esc; "closerequest" allows Esc + explicit close button */
  closedBy?: "any" | "closerequest" | "none";
  actions?: React.ReactNode;
  children: React.ReactNode;
};

export function ModalDialog({
  open,
  onClose,
  title,
  subtitle,
  large = false,
  closedBy = "any",
  actions,
  children
}: ModalDialogProps) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const titleId = useId();

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    if (open && !dialog.open) {
      dialog.showModal();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    const handleClose = () => {
      dialog.returnValue = "";
      onClose();
    };

    dialog.addEventListener("close", handleClose);

    // Fallback for browsers without native dialog `closedby` attribute support
    let cleanupLightDismiss: (() => void) | undefined;
    if (closedBy === "any" && !("closedBy" in HTMLDialogElement.prototype)) {
      let pressedOutside = false;
      const isOutside = (e: PointerEvent) => {
        const rect = dialog.getBoundingClientRect();
        return (
          e.clientX < rect.left ||
          e.clientX > rect.right ||
          e.clientY < rect.top ||
          e.clientY > rect.bottom
        );
      };
      const onPointerDown = (e: PointerEvent) => {
        pressedOutside = e.target === dialog && isOutside(e);
      };
      const onPointerUp = (e: PointerEvent) => {
        if (pressedOutside && e.target === dialog && isOutside(e)) {
          dialog.close();
        }
        pressedOutside = false;
      };
      dialog.addEventListener("pointerdown", onPointerDown);
      dialog.addEventListener("pointerup", onPointerUp);
      cleanupLightDismiss = () => {
        dialog.removeEventListener("pointerdown", onPointerDown);
        dialog.removeEventListener("pointerup", onPointerUp);
      };
    }

    return () => {
      dialog.removeEventListener("close", handleClose);
      cleanupLightDismiss?.();
    };
  }, [closedBy, onClose]);

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby={titleId}
      className={clsx("gemini-dialog", large && "large")}
      {...({ closedby: closedBy } as Record<string, string>)}
    >
      <div className="dialog-header">
        <div className="dialog-heading">
          <h2 id={titleId} className="dialog-title">
            {title}
          </h2>
          {subtitle ? <div className="dialog-subtitle">{subtitle}</div> : null}
        </div>
        <button
          type="button"
          className="icon-btn"
          onClick={() => dialogRef.current?.close()}
          aria-label="关闭对话框"
          title="关闭"
        >
          <X size={18} />
        </button>
      </div>
      <div className="dialog-body">{children}</div>
      {actions ? <div className="dialog-actions">{actions}</div> : null}
    </dialog>
  );
}
