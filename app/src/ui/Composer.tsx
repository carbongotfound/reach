// The one input: a large rounded box, controls in its bottom row, a round send / stop button.
import { useLayoutEffect, type ReactNode, type RefObject } from "react";
import { ArrowUp, Square } from "lucide-react";
import { S } from "../strings";

export function Composer({ value, onChange, onSubmit, onStop, inputRef, placeholder, label, disabled, canSend, running, controls, sendTitle }: {
  value: string; onChange: (v: string) => void; onSubmit: () => void; onStop?: () => void; inputRef: RefObject<HTMLTextAreaElement | null>;
  placeholder: string; label: string; disabled?: boolean; canSend: boolean; running?: boolean; controls?: ReactNode; sendTitle?: string;
}) {
  // grow with the text up to about 8 lines, then scroll
  useLayoutEffect(() => {
    const t = inputRef.current;
    if (!t) return;
    t.style.height = "auto";
    t.style.height = Math.min(t.scrollHeight, 220) + "px";
  }, [value]);
  return (
    <div className={`composer ${disabled ? "off" : ""}`} onClick={(e) => { if (e.target === e.currentTarget) inputRef.current?.focus(); }}>
      <textarea
        ref={inputRef}
        value={value}
        rows={1}
        aria-label={label}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); if (canSend) onSubmit(); } }}
      />
      <div className="composer-bar">
        {controls}
        <span className="grow" />
        {running && onStop ? (
          <button type="button" className="send stop" onClick={onStop} aria-label={S.composer.stop} data-tip={S.composer.stopTip} data-tip-pos="top">
            <Square size={14} fill="currentColor" aria-hidden />
          </button>
        ) : (
          <button type="button" className="send" onClick={onSubmit} disabled={!canSend} aria-label={S.composer.send} data-tip={sendTitle ?? S.composer.sendTip} data-tip-pos="top">
            <ArrowUp size={18} strokeWidth={2.25} aria-hidden />
          </button>
        )}
      </div>
    </div>
  );
}
