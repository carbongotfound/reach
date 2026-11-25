// An installed app's own icon, read from Windows (desk.rs app_icon) and cached; a neutral glyph when
// there is none. No third-party icons ship with Reach.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppWindow, type LucideIcon } from "lucide-react";

const cache = new Map<string, Promise<string | null>>();
const load = (key: string) => {
  let p = cache.get(key);
  if (!p) { p = invoke<string | null>("app_icon", { key }).catch(() => null); cache.set(key, p); }
  return p;
};

export function AppIcon({ appKey, size = 18, fallback: Fb = AppWindow }: { appKey?: string; size?: number; fallback?: LucideIcon }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    setSrc(null);
    if (!appKey) return;
    let on = true;
    load(appKey).then((s) => on && setSrc(s));
    return () => { on = false; };
  }, [appKey]);
  return src
    ? <img className="app-icon" src={src} width={size} height={size} alt="" aria-hidden draggable={false} />
    : <Fb className="app-icon fb" size={Math.round(size * 0.85)} strokeWidth={1.75} aria-hidden />;
}

/** The part `box` ([x, y, w, h] as fractions) of a screenshot, kept in proportion inside its box. */
export function Crop({ src, box, ar }: { src: string; box?: [number, number, number, number]; ar: number }) {
  const [x, y, w, h] = box ?? [0, 0, 1, 1];
  const pos = (o: number, s: number) => (s >= 1 ? 0 : (o / (1 - s)) * 100);
  return (
    <span className="crop">
      <span className="crop-in" style={{ ["--car" as string]: (w * ar) / h, backgroundImage: `url("${src}")`,
        backgroundSize: `${100 / w}% ${100 / h}%`, backgroundPosition: `${pos(x, w)}% ${pos(y, h)}%` }} />
    </span>
  );
}
