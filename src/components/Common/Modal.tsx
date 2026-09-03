import Icon from "../Icon";
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./Modal.css";

type Size = "small" | "medium" | "large" | "xlarge";

const HeaderSlot = createContext<HTMLDivElement | null>(null);

/** Кнопки в шапке модального окна (рендерятся из содержимого через портал). */
export function ModalActions({ children }: { children: ReactNode }) {
  const el = useContext(HeaderSlot);
  if (!el) return <>{children}</>;
  return createPortal(children, el);
}

/** Модальное окно. Закрывается по Esc и по клику на затемнение
 *  (а не по любому mousedown вне контента — иначе confirm/тултипы закрывали редактор). */
export default function Modal({ isOpen, onClose, title, children, size = "medium" }: { isOpen: boolean; onClose: () => void; title: ReactNode; children: ReactNode; size?: Size }) {
  const [slot, setSlot] = useState<HTMLDivElement | null>(null);
  useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", onKey);
    document.body.style.overflow = "hidden";
    return () => { document.removeEventListener("keydown", onKey); document.body.style.overflow = ""; };
  }, [isOpen, onClose]);
  if (!isOpen) return null;
  return (
    <div className="modal-overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className={`modal-content modal-${size}`}>
        <div className="modal-header">
          <h3>{title}</h3>
          <div className="modal-header-actions" ref={setSlot} />
          <button className="modal-close-btn" onClick={onClose}><Icon name="close"  /></button>
        </div>
        <HeaderSlot.Provider value={slot}>
          <div className="modal-body">{children}</div>
        </HeaderSlot.Provider>
      </div>
    </div>
  );
}
