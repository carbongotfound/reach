"""A test home with five short, realistic past tasks, for screenshots of the real app window.

    python -I tests/ui/seed-home.py <home>

Writes <home>/runs/<id>/run.json and ui.jsonl (the replay log) for each task. No screenshots, no
real window titles, no personal folders. Point the app at it with REACH_HOME=<home>.
"""
import json, os, sys
from datetime import datetime, timedelta

home = os.path.abspath(sys.argv[1])

def step(n, tool, action, args, ms, app="", ok=True, text=""):
    return {"type": "step", "n": n, "tool": tool, "action": action, "args": args, "ok": ok, "text": text, "ms": ms}

TASKS = [
    (1, 16, 20, "Open Excel and enter these three invoices into a table: Acme 1200, Northwind 850, Contoso 430. Then save it to the Desktop.", "done", "Excel", [
        step(1, "apps", "open", {"name": "Excel"}, 1840), step(2, "keyboard", "type", {"text": "Customer\tAmount"}, 420),
        step(3, "keyboard", "type", {"text": "Acme\t1200"}, 380), step(4, "keyboard", "type", {"text": "Northwind\t850"}, 390),
        step(5, "keyboard", "type", {"text": "Contoso\t430"}, 370), step(6, "keyboard", "key", {"text": "ctrl+s"}, 210)],
     "I entered the three invoices in a new sheet and saved it to the Desktop as invoices.xlsx."),
    (1, 11, 2, "Turn on dark mode in Windows Settings", "done", "Settings", [
        step(1, "apps", "open", {"name": "Settings"}, 1320), step(2, "ui", "find", {"query": "Colors"}, 64),
        step(3, "ui", "invoke", {"ref": "e4"}, 40), step(4, "ui", "set", {"ref": "e9", "value": "Dark"}, 52)],
     "Dark mode is on. Windows and apps now use the dark theme."),
    (3, 18, 2, "Rename my trip photos by the date they were taken", "stopped", "File Explorer", [
        step(1, "apps", "open", {"name": "File Explorer"}, 980), step(2, "ui", "inspect", {}, 70), step(3, "ui", "read", {"ref": "e12"}, 30)],
     None),
    (4, 17, 15, "Resize every image in the Screenshots folder to 1280 px wide", "failed", "Photos", [
        step(1, "apps", "open", {"name": "Photos"}, 1500),
        step(2, "ui", "find", {"query": "Resize image"}, 80, ok=False, text="Photos has no Resize command in this version, so I couldn't resize the images here.")],
     None),
    (6, 9, 40, "Work out 15% of 2480 in Calculator", "done", "Calculator", [
        step(1, "apps", "open", {"name": "Calculator"}, 900), step(2, "keyboard", "type", {"text": "2480*15%="}, 640), step(3, "ui", "read", {"ref": "e3"}, 25)],
     "15% of 2480 is 372."),
]

for days, h, m, task, state, app, steps, reply in TASKS:
    t0 = (datetime.now() - timedelta(days=days)).replace(hour=h, minute=m, second=4, microsecond=0)
    rid = t0.strftime("%Y%m%d-%H%M%S")
    d = os.path.join(home, "runs", rid)
    os.makedirs(d, exist_ok=True)
    ms = lambda k: int((t0 + timedelta(seconds=k)).timestamp() * 1000)
    evs = [{"type": "start", "id": rid, "task": task, "t": ms(0)}] + [dict(s, t=ms(4 + 3 * i)) for i, s in enumerate(steps)]
    if reply: evs.append({"type": "text", "text": reply, "t": ms(5 + 3 * len(steps))})
    if state == "failed": evs.append({"type": "status", "level": "error", "text": steps[-1]["text"], "t": ms(5 + 3 * len(steps))})
    evs.append({"type": "state", "state": state, "t": ms(6 + 3 * len(steps))})
    with open(os.path.join(d, "ui.jsonl"), "w", encoding="utf-8") as f:
        f.write("".join(json.dumps(e) + "\n" for e in evs))
    meta = {"id": rid, "task": task, "agent": "Claude", "started": t0.isoformat(), "ended": (t0 + timedelta(seconds=8 + 3 * len(steps))).isoformat(),
            "state": state, "steps": len(steps), "brain": "claude", "app": app, "appKey": ""}
    with open(os.path.join(d, "run.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=2)
print(f"seeded {len(TASKS)} tasks in {home}")
