import Icon from "./Icon";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./Tooltip.css";

/// `text` — строка или готовая разметка (например, с выделенными именами);
/// `inline` — обёртка без отступа и без курсора «?» (для капсул-бейджей).
export default function Tooltip({ text, children, inline = false }: { text: ReactNode; children?: ReactNode; inline?: boolean }) {
  const [show, setShow] = useState(false);
  const [pos, setPos] = useState({ top: 0, left: 0 });
  const ref = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useLayoutEffect(() => {
    if (show && ref.current) {
      const r = ref.current.getBoundingClientRect();
      const h = tipRef.current?.offsetHeight ?? 40;
      const w = tipRef.current?.offsetWidth ?? 200;
      // не выпускаем плашку за края окна
      const left = Math.min(Math.max(r.left + r.width / 2, w / 2 + 8), window.innerWidth - w / 2 - 8);
      setPos({ top: Math.max(8, r.top - h - 8), left });
    }
  }, [show]);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);

  return (
    <>
      <span
        className={inline ? "tooltip-inline" : "tooltip-container"}
        ref={ref}
        onMouseEnter={() => { timer.current = setTimeout(() => setShow(true), 200); }}
        onMouseLeave={() => { if (timer.current) clearTimeout(timer.current); setShow(false); }}
      >
        {children ?? <Icon name="help" className="tooltip-icon" />}
      </span>
      {show &&
        createPortal(
          <div className="tooltip-portal" style={{ position: "fixed", top: pos.top, left: pos.left, transform: "translateX(-50%)", zIndex: 99999, pointerEvents: "none" }}>
            <div className="tooltip-text-portal" ref={tipRef}>{text}</div>
          </div>,
          document.body,
        )}
    </>
  );
}
