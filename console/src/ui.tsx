import { useEffect, useRef, type ReactNode } from "react";
export function Icon({
  name,
}: {
  name:
    | "grid"
    | "plus"
    | "arrow"
    | "key"
    | "close"
    | "check"
    | "pulse"
    | "help"
    | "team";
}) {
  const paths = {
    grid: "M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z",
    plus: "M12 5v14 M5 12h14",
    arrow: "M5 12h14 M13 6l6 6-6 6",
    key: "M14 7a4 4 0 1 0-3 7L5 20H2v-3l6-6 M15 7h.01",
    close: "M6 6l12 12 M6 18L18 6",
    check: "M5 12l4 4L19 6",
    pulse: "M2 12h5l3-7 4 14 3-7h5",
    help: "M9 9a3 3 0 1 1 5 2c-2 1-2 2-2 3 M12 18h.01 M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0",
    team: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8 M22 21v-2a4 4 0 0 0-3-4 M16 3a4 4 0 0 1 0 8",
  };
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={paths[name]} />
    </svg>
  );
}
export function Dialog({
  title,
  children,
  onClose,
  busy = false,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  busy?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    ref.current?.showModal();
    return () => {
      ref.current?.close();
      prior?.focus();
    };
  }, []);
  return (
    <dialog
      ref={ref}
      aria-labelledby="dialog-title"
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) onClose();
      }}
    >
      <div className="dialog-head">
        <h2 id="dialog-title">{title}</h2>
        <button
          className="icon-button"
          type="button"
          aria-label="Close dialog"
          disabled={busy}
          onClick={onClose}
        >
          <Icon name="close" />
        </button>
      </div>
      {children}
    </dialog>
  );
}
export function FlowArt() {
  return (
    <svg
      className="flow-art"
      viewBox="0 0 290 170"
      role="img"
      aria-label="Illustration: connected sources flow into an organized project"
    >
      <rect
        x="1"
        y="1"
        width="288"
        height="168"
        rx="26"
        fill="var(--art-surface)"
      />
      <path
        d="M68 50h45q18 0 18 18v35q0 18 18 18h57 M68 121h25q20 0 20-20V71q0-21 21-21h72"
        fill="none"
        stroke="var(--art-line)"
        strokeWidth="2"
      />
      <rect
        x="35"
        y="29"
        width="46"
        height="44"
        rx="13"
        fill="var(--surface)"
        stroke="var(--border)"
      />
      <path
        d="M47 51h7l4-8 5 17 4-9h3"
        fill="none"
        stroke="var(--ink)"
        strokeWidth="2"
      />
      <rect
        x="35"
        y="99"
        width="46"
        height="44"
        rx="13"
        fill="var(--surface)"
        stroke="var(--border)"
      />
      <path
        d="m48 111 10 10-10 10m15 0h8"
        stroke="var(--ink)"
        strokeWidth="2"
        fill="none"
      />
      <rect
        x="183"
        y="33"
        width="69"
        height="104"
        rx="18"
        fill="var(--accent)"
      />
      <path
        d="M201 59h33 M201 70h23 M201 91h33 M201 102h18"
        stroke="var(--surface)"
        strokeWidth="3"
        strokeLinecap="round"
      />
      <circle cx="132" cy="85" r="14" fill="var(--lime)" />
      <path d="m126 85 4 4 8-9" fill="none" stroke="#18382a" strokeWidth="2" />
    </svg>
  );
}
