// Reach: small, fast Windows desktop control for AI agents.
// One exe: tray server, CLI, and MCP (streamable HTTP on 127.0.0.1). MIT licensed.

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Net;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Windows.Automation;
using System.Windows.Forms;
using System.Web.Script.Serialization;

static class Paths
{
    public const string Version = "0.2.0";
    // REACH_HOME lets you run a second, isolated instance (own config, token, port).
    public static readonly string Dir = !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("REACH_HOME"))
        ? Environment.GetEnvironmentVariable("REACH_HOME")
        : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".reach");
    public static readonly string Token = Path.Combine(Dir, "token");
    public static readonly string Usage = Path.Combine(Dir, "usage.jsonl");
    public static readonly string Log = Path.Combine(Dir, "reach.log");
    public static readonly string Config = Path.Combine(Dir, "config.json");
    public static readonly string LastShot = Path.Combine(Dir, "last.jpg");
    public static readonly string LastZoom = Path.Combine(Dir, "zoom.jpg");
    public const int DefaultPort = 7420;
    public const int MaxDim = 1280;
}

static class Log
{
    public static void Write(string s)
    {
        try
        {
            Directory.CreateDirectory(Paths.Dir);
            File.AppendAllText(Paths.Log, DateTime.Now.ToString("s") + " " + s + Environment.NewLine);
        }
        catch { }
    }
}

static class Usage
{
    static readonly JavaScriptSerializer Json = new JavaScriptSerializer();

    public static void Record(string tool, string action, int ms, bool ok, string detail)
    {
        try
        {
            Directory.CreateDirectory(Paths.Dir);
            var row = new Dictionary<string, object>();
            row["ts"] = DateTime.Now.ToString("s");
            row["tool"] = tool;
            row["action"] = action ?? "";
            row["ms"] = ms;
            row["ok"] = ok;
            if (!string.IsNullOrEmpty(detail) && detail.Length > 120)
                detail = detail.Substring(0, 117) + "...";
            if (!string.IsNullOrEmpty(detail)) row["detail"] = detail;
            File.AppendAllText(Paths.Usage, Json.Serialize(row) + Environment.NewLine);
        }
        catch { }
    }

    public static string Tail(int n)
    {
        if (!File.Exists(Paths.Usage)) return "(no usage yet)";
        var lines = File.ReadAllLines(Paths.Usage);
        int start = Math.Max(0, lines.Length - n);
        var sb = new StringBuilder();
        sb.AppendLine(lines.Length + " recorded call(s), last " + Math.Min(n, lines.Length) + ":");
        for (int i = start; i < lines.Length; i++) sb.AppendLine(lines[i]);
        return sb.ToString().TrimEnd();
    }
}

sealed class Config
{
    public int Port = Paths.DefaultPort;
    public int MaxDim = Paths.MaxDim;
    public bool Paused;
    public bool AgentCursor = true;
    public bool CursorRipple = true;
    public bool PauseOnUser = true;
    public int UserPauseIdleMs = 2000;
    public int UserPauseTimeoutMs = 60000;
    public string Motion = "smooth";   // smooth | fast | instant
    public string Token = "";

    public static Config Current = new Config();

    public static void Load()
    {
        Directory.CreateDirectory(Paths.Dir);
        Current = new Config();
        if (File.Exists(Paths.Token))
            Current.Token = File.ReadAllText(Paths.Token).Trim();
        if (string.IsNullOrEmpty(Current.Token))
        {
            var bytes = new byte[32];
            using (var rng = RandomNumberGenerator.Create()) rng.GetBytes(bytes);
            var sb = new StringBuilder(64);
            for (int i = 0; i < bytes.Length; i++) sb.Append(bytes[i].ToString("x2"));
            Current.Token = sb.ToString();
            File.WriteAllText(Paths.Token, Current.Token);
        }
        bool needSave = !File.Exists(Paths.Config);
        if (!needSave)
        {
            try
            {
                var ser = new JavaScriptSerializer();
                var d = ser.Deserialize<Dictionary<string, object>>(File.ReadAllText(Paths.Config));
                if (d != null)
                {
                    if (d.ContainsKey("port")) Current.Port = Convert.ToInt32(d["port"]);
                    if (d.ContainsKey("maxDim")) Current.MaxDim = Convert.ToInt32(d["maxDim"]);
                    if (d.ContainsKey("paused")) Current.Paused = Convert.ToBoolean(d["paused"]);
                    if (d.ContainsKey("agentCursor")) Current.AgentCursor = Convert.ToBoolean(d["agentCursor"]);
                    if (d.ContainsKey("cursorRipple")) Current.CursorRipple = Convert.ToBoolean(d["cursorRipple"]);
                    if (d.ContainsKey("pauseOnUser")) Current.PauseOnUser = Convert.ToBoolean(d["pauseOnUser"]);
                    if (d.ContainsKey("userPauseIdleMs")) Current.UserPauseIdleMs = Convert.ToInt32(d["userPauseIdleMs"]);
                    if (d.ContainsKey("userPauseTimeoutMs")) Current.UserPauseTimeoutMs = Convert.ToInt32(d["userPauseTimeoutMs"]);
                    if (d.ContainsKey("motion")) Current.Motion = Convert.ToString(d["motion"]);
                    foreach (var k in new[] { "port", "maxDim", "paused", "agentCursor", "cursorRipple", "pauseOnUser", "userPauseIdleMs", "userPauseTimeoutMs", "motion" })
                        if (!d.ContainsKey(k)) needSave = true;
                }
            }
            catch { needSave = true; }
        }
        if (needSave) Save();
    }

    public static void Save()
    {
        try
        {
            var ser = new JavaScriptSerializer();
            var d = new Dictionary<string, object>();
            d["port"] = Current.Port;
            d["maxDim"] = Current.MaxDim;
            d["paused"] = Current.Paused;
            d["agentCursor"] = Current.AgentCursor;
            d["cursorRipple"] = Current.CursorRipple;
            d["pauseOnUser"] = Current.PauseOnUser;
            d["userPauseIdleMs"] = Current.UserPauseIdleMs;
            d["userPauseTimeoutMs"] = Current.UserPauseTimeoutMs;
            d["motion"] = Current.Motion;
            File.WriteAllText(Paths.Config, ser.Serialize(d));
        }
        catch { }
    }
}

static class Native
{
    public const int INPUT_MOUSE = 0;
    public const int INPUT_KEYBOARD = 1;
    public const uint MOUSEEVENTF_MOVE = 0x0001;
    public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP = 0x0004;
    public const uint MOUSEEVENTF_RIGHTDOWN = 0x0008;
    public const uint MOUSEEVENTF_RIGHTUP = 0x0010;
    public const uint MOUSEEVENTF_MIDDLEDOWN = 0x0020;
    public const uint MOUSEEVENTF_MIDDLEUP = 0x0040;
    public const uint MOUSEEVENTF_WHEEL = 0x0800;
    public const uint MOUSEEVENTF_ABSOLUTE = 0x8000;
    public const uint MOUSEEVENTF_VIRTUALDESK = 0x4000;
    public const uint KEYEVENTF_KEYUP = 0x0002;
    public const uint KEYEVENTF_UNICODE = 0x0004;
    public const uint KEYEVENTF_SCANCODE = 0x0008;
    public const uint KEYEVENTF_EXTENDEDKEY = 0x0001;
    public const int SW_RESTORE = 9;
    public const int SW_SHOWMINIMIZED = 2;
    public const int SW_SHOWMAXIMIZED = 3;
    public const int SW_SHOWNOACTIVATE = 4;
    public const int GWL_EXSTYLE = -20;
    public const int WS_EX_TOOLWINDOW = 0x00000080;
    public const int WS_EX_NOACTIVATE = 0x08000000;

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT
    {
        public int type;
        public INPUTUNION U;
    }

    [StructLayout(LayoutKind.Explicit)]
    public struct INPUTUNION
    {
        [FieldOffset(0)] public MOUSEINPUT mi;
        [FieldOffset(0)] public KEYBDINPUT ki;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MOUSEINPUT
    {
        public int dx, dy;
        public uint mouseData, dwFlags, time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT
    {
        public ushort wVk, wScan;
        public uint dwFlags, time;
        public IntPtr dwExtraInfo;
    }

    public delegate bool EnumProc(IntPtr h, IntPtr l);

    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll", SetLastError = true)] public static extern uint SendInput(uint n, INPUT[] i, int cb);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("winmm.dll")] public static extern uint timeBeginPeriod(uint p);
    [DllImport("winmm.dll")] public static extern uint timeEndPeriod(uint p);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc lp, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool f);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr none);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("kernel32.dll")] public static extern bool FreeConsole();
    [DllImport("kernel32.dll")] public static extern bool AttachConsole(int pid);
    [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();

    public static void Send(INPUT[] inputs)
    {
        if (inputs == null || inputs.Length == 0) return;
        uint n = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT)));
        if (n == 0) throw new InvalidOperationException("SendInput failed (" + Marshal.GetLastWin32Error() + ")");
    }

    public static string TitleOf(IntPtr h)
    {
        int len = GetWindowTextLength(h);
        if (len <= 0) return "";
        var sb = new StringBuilder(len + 1);
        GetWindowText(h, sb, sb.Capacity);
        return sb.ToString();
    }

    public static void ForceFocus(IntPtr h)
    {
        if (h == IntPtr.Zero || !IsWindow(h)) return;
        if (IsIconic(h)) ShowWindow(h, SW_RESTORE);
        uint fg = GetWindowThreadProcessId(GetForegroundWindow(), IntPtr.Zero);
        uint self = GetCurrentThreadId();
        if (fg != self) AttachThreadInput(self, fg, true);
        BringWindowToTop(h);
        SetForegroundWindow(h);
        if (fg != self) AttachThreadInput(self, fg, false);
    }
}

sealed class ScreenMap
{
    public Rectangle Virtual;
    public double Scale = 1;
    public int OutW, OutH;
    public int MaxDim;

    public static ScreenMap Build(int maxDim)
    {
        var vs = SystemInformation.VirtualScreen;
        var m = new ScreenMap();
        m.Virtual = vs;
        m.MaxDim = maxDim;
        int longEdge = Math.Max(vs.Width, vs.Height);
        m.Scale = longEdge > maxDim ? (double)maxDim / longEdge : 1.0;
        m.OutW = Math.Max(1, (int)Math.Round(vs.Width * m.Scale));
        m.OutH = Math.Max(1, (int)Math.Round(vs.Height * m.Scale));
        return m;
    }

    public Point ToScreen(int x, int y)
    {
        int sx = Virtual.X + (int)Math.Round(x / Scale);
        int sy = Virtual.Y + (int)Math.Round(y / Scale);
        return new Point(sx, sy);
    }

    public Point ToImage(int screenX, int screenY)
    {
        int x = (int)Math.Round((screenX - Virtual.X) * Scale);
        int y = (int)Math.Round((screenY - Virtual.Y) * Scale);
        return new Point(x, y);
    }

}

sealed class ActResult
{
    public bool Ok = true;
    public string Text = "ok";
    public byte[] Image;
    public string Mime = "image/jpeg";
    public string ImagePath;

    public static ActResult Say(string text) { return new ActResult { Text = text ?? "ok" }; }
    public static ActResult Fail(string text) { return new ActResult { Ok = false, Text = text }; }
}

sealed class ElementStore
{
    // Refs stay valid across finds (same element => same ref) so the next command
    // can target an element without re-reading the tree.
    readonly Dictionary<string, AutomationElement> map = new Dictionary<string, AutomationElement>();
    readonly Dictionary<string, string> byRuntime = new Dictionary<string, string>();
    int next = 1;

    public static bool IsRef(string s)
    {
        if (string.IsNullOrEmpty(s) || s.Length < 2 || (s[0] != 'e' && s[0] != 'E')) return false;
        for (int i = 1; i < s.Length; i++) if (!char.IsDigit(s[i])) return false;
        return true;
    }

    public string Add(AutomationElement el)
    {
        string key = null;
        try { key = string.Join(".", el.GetRuntimeId()); } catch { }
        string id;
        if (key != null && byRuntime.TryGetValue(key, out id)) { map[id] = el; return id; }
        if (map.Count > 3000) { map.Clear(); byRuntime.Clear(); next = 1; }
        id = "e" + next++;
        map[id] = el;
        if (key != null) byRuntime[key] = id;
        return id;
    }

    public AutomationElement Get(string id)
    {
        AutomationElement el;
        if (id == null || !map.TryGetValue(id.ToLowerInvariant(), out el) || el == null)
            throw new Exception("Unknown ref `" + id + "`. Run ui find or ui inspect first.");
        try { var _ = el.Current.ProcessId; }
        catch { throw new Exception("Ref `" + id + "` is gone (the control closed or changed). Run ui find again."); }
        return el;
    }
}

sealed class Engine
{
    public readonly object Gate = new object();
    public readonly ElementStore Elements = new ElementStore();
    string lastHash = "";
    SynchronizationContext ui;

    public Engine(SynchronizationContext uiCtx) { ui = uiCtx; }

    public ActResult Run(string tool, Dictionary<string, object> args)
    {
        if (args == null) args = new Dictionary<string, object>();
        string action = Str(args, "action", "");
        if (Config.Current.Paused && !IsPauseExempt(tool, action))
            return ActResult.Fail("Reach is paused. Resume it from the tray icon, or run `reach resume`.");

        var sw = Stopwatch.StartNew();
        ActResult r;
        int yielded = 0;
        try
        {
            if (!Bool(args, "no_yield", false) && UserYield.Gated(tool, action))
                yielded = UserYield.WaitForIdle(AgentCursor.Resolve(args, null));
            if (ui != null && SynchronizationContext.Current != ui)
            {
                ActResult inner = null;
                Exception ex = null;
                ui.Send(delegate
                {
                    try { inner = DispatchAs(tool, action, args); }
                    catch (Exception e) { ex = e; }
                }, null);
                if (ex != null) throw ex;
                r = inner ?? ActResult.Fail("no result");
            }
            else r = DispatchAs(tool, action, args);
        }
        catch (Exception ex)
        {
            r = ActResult.Fail(ex.Message);
        }
        sw.Stop();
        if (yielded > 0 && r.Ok) r.Text = (r.Text ?? "ok") + "  (waited " + (yielded / 1000.0).ToString("0.0") + " s for you to stop using the PC)";
        Usage.Record(tool, action, (int)sw.ElapsedMilliseconds, r.Ok,
            r.Ok ? Short(r.Text) : Short(r.Text));
        return r;
    }

    [ThreadStatic] static bool NoYieldThisCall;
    [ThreadStatic] static bool FastThisCall;
    static readonly Random rng = new Random();
    static double Rand() { lock (rng) return rng.NextDouble(); }

    // smooth (default): eased, curved glides and paced typing. fast: short glides, bulk typing. instant: teleport.
    static string MotionMode()
    {
        string m = (Config.Current.Motion ?? "smooth").ToLowerInvariant();
        if (m != "fast" && m != "instant") m = "smooth";
        if (FastThisCall && m == "smooth") m = "fast";
        return m;
    }

    ActResult DispatchAs(string tool, string action, Dictionary<string, object> args)
    {
        AgentCursor.Current = AgentCursor.Resolve(args, null);
        AgentCursor.NoCursorThisCall = Bool(args, "no_cursor", false);
        NoYieldThisCall = Bool(args, "no_yield", false);
        FastThisCall = Bool(args, "fast", false);
        try { return Dispatch(tool, action, args); }
        finally { AgentCursor.NoCursorThisCall = false; NoYieldThisCall = false; FastThisCall = false; }
    }

    static bool IsPauseExempt(string tool, string action)
    {
        if (tool == "status" || tool == "usage" || tool == "pause_on_user" || tool == "motion") return true;
        if (tool == "apps" && (action == "resume" || action == "status")) return true;
        return action == "resume" || action == "status" || action == "usage";
    }

    ActResult Dispatch(string tool, string action, Dictionary<string, object> a)
    {
        switch ((tool ?? "").ToLowerInvariant())
        {
            case "screenshot": return Shot(a);
            case "mouse": return Mouse(action, a);
            case "keyboard": return Keyboard(action, a);
            case "apps": return Apps(action, a);
            case "ui": return Ui(action, a);
            case "status": return Status(Bool(a, "json", false));
            case "cursor": return CursorCmd(action, a);
            case "pause_on_user": return PauseOnUserCmd(action, a);
            case "motion":
                {
                    string v = Str(a, "value", Str(a, "action", "")).ToLowerInvariant();
                    if (v == "smooth" || v == "fast" || v == "instant") { Config.Current.Motion = v; Config.Save(); }
                    else if (v != "" && v != "status") return ActResult.Fail("motion is smooth, fast or instant");
                    return ActResult.Say("motion " + MotionMode());
                }
            case "usage": return ActResult.Say(Usage.Tail(Int(a, "limit", 20)));
            case "pause":
                Config.Current.Paused = true; Config.Save();
                return ActResult.Say("paused");
            case "resume":
                Config.Current.Paused = false; Config.Save();
                return ActResult.Say("resumed");
            default:
                return ActResult.Fail("Unknown tool `" + tool + "`. Use screenshot, mouse, keyboard, apps, ui, status, usage, pause, resume.");
        }
    }

    ActResult PauseOnUserCmd(string action, Dictionary<string, object> a)
    {
        int v = Int(a, "value", 0);
        switch ((action ?? "status").ToLowerInvariant())
        {
            case "on": Config.Current.PauseOnUser = true; Config.Save(); break;
            case "off": Config.Current.PauseOnUser = false; Config.Save(); break;
            case "idle": if (v < 100) throw new Exception("usage: reach pause-on-user idle <ms>"); Config.Current.UserPauseIdleMs = v; Config.Save(); break;
            case "timeout": if (v < 1000) throw new Exception("usage: reach pause-on-user timeout <ms> (min 1000)"); Config.Current.UserPauseTimeoutMs = v; Config.Save(); break;
            case "simulate": UserYield.Simulate(v > 0 ? v : 1000); return ActResult.Say("simulating your input for " + (v > 0 ? v : 1000) + " ms");
            case "status": break;
            default: throw new Exception("usage: reach pause-on-user on|off|status|idle <ms>|timeout <ms>");
        }
        return ActResult.Say(PauseOnUserStatus());
    }

    static string PauseOnUserStatus()
    {
        int idle = UserYield.IdleMs;
        return "pause-on-user " + (Config.Current.PauseOnUser ? "on" : "off")
            + "  idle " + Config.Current.UserPauseIdleMs + " ms  timeout " + Config.Current.UserPauseTimeoutMs + " ms"
            + "  hooks " + (UserYield.HooksOk ? "ok" : "MISSING")
            + "  input-lock " + (UserYield.InputLockRunning ? "running (ignored)" : "off")
            + "  you were last active " + (idle > 3600000 ? "over an hour" : (idle / 1000.0).ToString("0.0") + " s") + " ago";
    }

    ActResult CursorCmd(string action, Dictionary<string, object> a)
    {
        switch ((action ?? "status").ToLowerInvariant())
        {
            case "ripple":
                {
                    string v = Str(a, "value", "").ToLowerInvariant();
                    if (v == "on" || v == "off") { Config.Current.CursorRipple = v == "on"; Config.Save(); }
                    return ActResult.Say("click ripple " + (Config.Current.CursorRipple ? "on" : "off"));
                }
            case "on": case "enable":
                Config.Current.AgentCursor = true; Config.Save();
                return ActResult.Say("agent cursor on");
            case "off": case "disable":
                Config.Current.AgentCursor = false; Config.Save();
                AgentCursor.HideAll();
                return ActResult.Say("agent cursor off");
            case "capture":
                {
                    // Debug: let screen recorders see the pill (off by default).
                    string v = Str(a, "value", "").ToLowerInvariant();
                    AgentCursor.SetCaptureVisible(v == "on");
                    return ActResult.Say("pill visible to capture: " + (v == "on" ? "yes" : "no"));
                }
            case "show": case "test":
                {
                    string k = Str(a, "value", "click").ToLowerInvariant();
                    AgentCursor.AtCursor(k == "type" || k == "move" ? k : "click");
                }
                return ActResult.Say("showing agent cursor for " + AgentCursor.Current);
            default:
                return ActResult.Say("agent cursor " + (Config.Current.AgentCursor ? "on" : "off") + ", ripple " + (Config.Current.CursorRipple ? "on" : "off"));
        }
    }

    public ActResult Status() { return Status(false); }

    public ActResult Status(bool json)
    {
        var p = Process.GetCurrentProcess();
        var map = ScreenMap.Build(Config.Current.MaxDim);
        if (json)
        {
            var d = new Dictionary<string, object>();
            d["version"] = Paths.Version; d["paused"] = Config.Current.Paused; d["port"] = Config.Current.Port;
            d["cursor"] = Config.Current.AgentCursor; d["pauseOnUser"] = Config.Current.PauseOnUser;
            d["image"] = map.OutW + "x" + map.OutH; d["displays"] = Screen.AllScreens.Length;
            d["pid"] = p.Id; d["ramKB"] = p.WorkingSet64 / 1024;
            return ActResult.Say(new JavaScriptSerializer().Serialize(d));
        }
        var sb = new StringBuilder();
        sb.Append("reach ").Append(Paths.Version).Append(Config.Current.Paused ? " paused" : " running")
          .Append(" port ").Append(Config.Current.Port)
          .Append(" cursor ").Append(Config.Current.AgentCursor ? "on" : "off")
          .Append(" yield ").Append(Config.Current.PauseOnUser ? "on" : "off")
          .Append(" image ").Append(map.OutW).Append('x').Append(map.OutH)
          .Append(" displays ").Append(Screen.AllScreens.Length)
          .Append(" pid ").Append(p.Id).Append(' ').Append(p.WorkingSet64 / (1024 * 1024)).Append("MB");
        return ActResult.Say(sb.ToString());
    }

    ActResult Shot(Dictionary<string, object> a)
    {
        bool force = Bool(a, "force", false);
        int[] region = Ints(a, "region");
        string window = Str(a, "window", "");
        int maxDim = Math.Max(200, Math.Min(4096, Int(a, "max", Config.Current.MaxDim)));
        long quality = Math.Max(20, Math.Min(95, Int(a, "quality", 0)));
        var map = ScreenMap.Build(Config.Current.MaxDim);
        var outMap = maxDim == Config.Current.MaxDim ? map : ScreenMap.Build(maxDim);
        Rectangle src;
        if (region != null && region.Length == 4)
        {
            Point tl = map.ToScreen(region[0], region[1]);
            int w = Math.Max(1, (int)Math.Round(region[2] / map.Scale));
            int h = Math.Max(1, (int)Math.Round(region[3] / map.Scale));
            src = new Rectangle(tl, new Size(w, h));
        }
        else if (!string.IsNullOrEmpty(window))
        {
            IntPtr h = ParseHwnd(window);
            Native.RECT r;
            if (!Native.GetWindowRect(h, out r))
                return ActResult.Fail("Could not get window bounds.");
            src = Rectangle.FromLTRB(r.Left, r.Top, r.Right, r.Bottom);
        }
        else
        {
            src = map.Virtual;
        }

        src = Rectangle.Intersect(src, Rectangle.FromLTRB(
            map.Virtual.X - 10000, map.Virtual.Y - 10000,
            map.Virtual.Right + 10000, map.Virtual.Bottom + 10000));
        if (src.Width < 1 || src.Height < 1)
            return ActResult.Fail("Empty capture region.");

        int outW, outH;
        bool full = (src == map.Virtual) && region == null;
        if (full) { outW = outMap.OutW; outH = outMap.OutH; }
        else
        {
            double sc = Math.Min(1.0, (double)Config.Current.MaxDim / Math.Max(src.Width, src.Height));
            if (region != null) sc = Math.Min(2.0, Math.Max(sc, 1.0)); // zoom a bit when asked for a region
            // For a zoom/region, keep more detail: long edge up to MaxDim
            sc = (double)Config.Current.MaxDim / Math.Max(src.Width, src.Height);
            if (sc > 2) sc = 2;
            if (sc > 1 && src.Width * src.Height > 0) { /* allow upsample only if tiny */ if (Math.Max(src.Width, src.Height) > 400) sc = Math.Min(sc, 1); }
            outW = Math.Max(1, (int)Math.Round(src.Width * Math.Min(sc, 1)));
            outH = Math.Max(1, (int)Math.Round(src.Height * Math.Min(sc, 1)));
            if (Math.Max(src.Width, src.Height) > Config.Current.MaxDim)
            {
                double s2 = (double)Config.Current.MaxDim / Math.Max(src.Width, src.Height);
                outW = Math.Max(1, (int)Math.Round(src.Width * s2));
                outH = Math.Max(1, (int)Math.Round(src.Height * s2));
            }
            else { outW = src.Width; outH = src.Height; }
        }

        byte[] jpeg;
        bool withOverlay = Bool(a, "include_overlay", false);
        using (var bmp = new Bitmap(src.Width, src.Height, PixelFormat.Format24bppRgb))
        {
            if (withOverlay) { AgentCursor.SetCaptureVisible(true); Thread.Sleep(120); }
            try
            {
                using (var g = Graphics.FromImage(bmp))
                    g.CopyFromScreen(src.Location, Point.Empty, src.Size, CopyPixelOperation.SourceCopy);
            }
            finally { if (withOverlay) AgentCursor.SetCaptureVisible(false); }
            using (var scaled = (outW == src.Width && outH == src.Height) ? bmp : new Bitmap(bmp, outW, outH))
            {
                jpeg = ToJpeg(scaled, Int(a, "quality", 0) > 0 ? quality : (region != null) ? (long)80 : (long)72);
                if (scaled != bmp) scaled.Dispose();
            }
        }

        string hash = Sha(jpeg);
        if (!force && !withOverlay && hash == lastHash)
            return ActResult.Say("unchanged " + outW + "x" + outH);
        lastHash = hash;

        string path = region != null ? Paths.LastZoom : Paths.LastShot;
        File.WriteAllBytes(path, jpeg);

        Native.POINT cur;
        Native.GetCursorPos(out cur);
        Point ic = map.ToImage(cur.X, cur.Y);

        var text = new StringBuilder();
        text.Append(outW).Append('x').Append(outH).Append(full ? "" : " region");
        if (full && outMap != map) text.Append(" (scaled; click coords use ").Append(map.OutW).Append('x').Append(map.OutH).Append(" space)");
        text.Append(" cursor ").Append(ic.X).Append(',').Append(ic.Y);
        if (full)
        {
            foreach (var sc in Screen.AllScreens)
            {
                var tl = map.ToImage(sc.Bounds.X, sc.Bounds.Y);
                var br = map.ToImage(sc.Bounds.Right, sc.Bounds.Bottom);
                text.Append(" | ").Append(tl.X).Append(',').Append(tl.Y).Append(' ')
                    .Append(br.X - tl.X).Append('x').Append(br.Y - tl.Y).Append(sc.Primary ? "*" : "");
            }
        }
        return new ActResult { Text = text.ToString(), Image = jpeg, Mime = "image/jpeg", ImagePath = path };
    }

    ActResult Mouse(string action, Dictionary<string, object> a)
    {
        if (string.IsNullOrEmpty(action)) action = Str(a, "action", "click");
        action = action.ToLowerInvariant().Replace("-", "_");
        var map = ScreenMap.Build(Config.Current.MaxDim);
        int[] c = Ints(a, "coordinate");
        if (c == null)
        {
            int x, y;
            if (TryInt(a, "x", out x) && TryInt(a, "y", out y))
                c = new[] { x, y };
        }

        switch (action)
        {
            case "position":
            case "cursor_position":
                Native.POINT p;
                Native.GetCursorPos(out p);
                Point ip = map.ToImage(p.X, p.Y);
                return ActResult.Say(ip.X + "," + ip.Y);
            case "move":
            case "mouse_move":
                NeedCoord(c);
                MoveTo(map.ToScreen(c[0], c[1]));
                AgentCursor.At(map.ToScreen(c[0], c[1]), "move");
                return ActResult.Say("ok");
            case "click":
            case "left_click":
                NeedCoord(c);
                return Click(map, c[0], c[1], "left", 1);
            case "right_click":
                NeedCoord(c);
                return Click(map, c[0], c[1], "right", 1);
            case "middle_click":
                NeedCoord(c);
                return Click(map, c[0], c[1], "middle", 1);
            case "double_click":
            case "doubleclick":
                NeedCoord(c);
                return Click(map, c[0], c[1], "left", 2);
            case "triple_click":
                NeedCoord(c);
                return Click(map, c[0], c[1], "left", 3);
            case "drag":
            case "left_click_drag":
                NeedCoord(c);
                int[] to = Ints(a, "to");
                if (to == null || to.Length != 2) throw new Exception("`to` [x,y] is required for drag.");
                return Drag(map, c[0], c[1], to[0], to[1]);
            case "scroll":
                {
                    string dir = Str(a, "direction", Str(a, "scroll_direction", "down")).ToLowerInvariant();
                    int amount = Int(a, "amount", Int(a, "scroll_amount", 3));
                    if (c != null && c.Length == 2) MoveTo(map.ToScreen(c[0], c[1]));
                    AgentCursor.AtCursor("move");
                    int delta = 120 * Math.Max(1, amount);
                    if (dir == "down" || dir == "right") { /* wheel down is negative */ delta = -delta; }
                    else if (dir == "up" || dir == "left") { }
                    else throw new Exception("direction must be up, down, left or right.");
                    uint flags = (dir == "left" || dir == "right") ? 0x1000 /*HWHEEL*/ : Native.MOUSEEVENTF_WHEEL;
                    var inp = MouseInput(0, 0, unchecked((uint)(int)delta), flags);
                    Native.Send(new[] { inp });
                    return ActResult.Say("ok");
                }
            case "down":
            case "left_mouse_down":
                if (c != null && c.Length == 2) MoveTo(map.ToScreen(c[0], c[1]));
                AgentCursor.AtCursor("click");
                Native.Send(new[] { MouseInput(0, 0, 0, Native.MOUSEEVENTF_LEFTDOWN) });
                return ActResult.Say("ok");
            case "up":
            case "left_mouse_up":
                Native.Send(new[] { MouseInput(0, 0, 0, Native.MOUSEEVENTF_LEFTUP) });
                return ActResult.Say("ok");
            case "wait":
                {
                    double sec = Num(a, "duration", 0.5);
                    if (sec < 0) sec = 0;
                    if (sec > 10) sec = 10;
                    Thread.Sleep((int)(sec * 1000));
                    return ActResult.Say("ok");
                }
            default:
                return ActResult.Fail("Unknown mouse action `" + action + "`.");
        }
    }

    ActResult Click(ScreenMap map, int x, int y, string button, int times)
    {
        bool smooth = MotionMode() == "smooth";
        MoveTo(map.ToScreen(x, y));
        AgentCursor.At(map.ToScreen(x, y), "click");
        Thread.Sleep(smooth ? 35 : 15);
        uint down, up;
        if (button == "right") { down = Native.MOUSEEVENTF_RIGHTDOWN; up = Native.MOUSEEVENTF_RIGHTUP; }
        else if (button == "middle") { down = Native.MOUSEEVENTF_MIDDLEDOWN; up = Native.MOUSEEVENTF_MIDDLEUP; }
        else { down = Native.MOUSEEVENTF_LEFTDOWN; up = Native.MOUSEEVENTF_LEFTUP; }
        for (int i = 0; i < times; i++)
        {
            Native.Send(new[] { MouseInput(0, 0, 0, down) });
            try { Thread.Sleep(smooth ? 45 + (int)(Rand() * 30) : 8); }   // press/release gap
            finally { Native.Send(new[] { MouseInput(0, 0, 0, up) }); }
            if (i + 1 < times) Thread.Sleep(smooth ? 70 : 40);
        }
        return ActResult.Say("ok");
    }

    ActResult Drag(ScreenMap map, int x1, int y1, int x2, int y2)
    {
        bool smooth = MotionMode() == "smooth";
        MoveTo(map.ToScreen(x1, y1));
        AgentCursor.At(map.ToScreen(x1, y1), "click");
        Thread.Sleep(smooth ? 60 : 30);
        Native.Send(new[] { MouseInput(0, 0, 0, Native.MOUSEEVENTF_LEFTDOWN) });
        try
        {
            Thread.Sleep(smooth ? 70 : 30);
            MoveTo(map.ToScreen(x2, y2), 1.5);
            AgentCursor.At(map.ToScreen(x2, y2), "move");
            Thread.Sleep(smooth ? 70 : 30);
        }
        finally { Native.Send(new[] { MouseInput(0, 0, 0, Native.MOUSEEVENTF_LEFTUP) }); }
        return ActResult.Say("ok");
    }

    static void MoveTo(Point screen) { MoveTo(screen, 1.0); }

    // Glide the real cursor along an eased, slightly curved path (~120 Hz), then land
    // exactly on the target and verify. If you grab the mouse mid-glide, it stops,
    // waits for you, and resumes from wherever the cursor is.
    static void MoveTo(Point target, double durScale)
    {
        string mode = MotionMode();
        Native.POINT c;
        if (mode != "instant" && Native.GetCursorPos(out c))
        {
            double dx = target.X - c.X, dy = target.Y - c.Y, d = Math.Sqrt(dx * dx + dy * dy);
            if (d >= 3)
            {
                double ms = mode == "fast" ? Math.Min(120, 50 + d * 0.06) : Math.Min(450, 150 + d * 0.22);
                ms *= durScale;
                double off = (0.5 + Rand() * 0.5) * Math.Min(60, d * 0.12) * (Rand() < 0.5 ? -1 : 1);
                if (mode == "fast") off *= 0.4;
                double kx = (c.X + target.X) / 2.0 - dy / d * off, ky = (c.Y + target.Y) / 2.0 + dx / d * off;
                bool interrupted = false;
                Native.timeBeginPeriod(1);
                try
                {
                    var sw = Stopwatch.StartNew();
                    int lx = int.MinValue, ly = int.MinValue;
                    while (true)
                    {
                        double t = Math.Min(1.0, sw.Elapsed.TotalMilliseconds / ms);
                        double e = t < 0.5 ? 4 * t * t * t : 1 - Math.Pow(-2 * t + 2, 3) / 2;   // ease-in-out
                        double u = 1 - e;
                        int ix = (int)Math.Round(u * u * c.X + 2 * u * e * kx + e * e * target.X);
                        int iy = (int)Math.Round(u * u * c.Y + 2 * u * e * ky + e * e * target.Y);
                        if (ix != lx || iy != ly) { SendAbs(ix, iy); lx = ix; ly = iy; }
                        if (t >= 1) break;
                        if (!NoYieldThisCall && Config.Current.PauseOnUser && UserYield.IdleMs < 120) { interrupted = true; break; }
                        Thread.Sleep(8);
                    }
                }
                finally { Native.timeEndPeriod(1); }
                if (interrupted)
                {
                    UserYield.WaitForIdle(AgentCursor.Current);
                    MoveTo(target, durScale);
                    return;
                }
            }
        }
        SendAbs(target.X, target.Y);
        // Re-verify the landing spot; absolute coordinates can round off by a pixel.
        Native.POINT f;
        if (Native.GetCursorPos(out f) && (f.X != target.X || f.Y != target.Y))
            Native.SetCursorPos(target.X, target.Y);
    }

    static void SendAbs(int x, int y)
    {
        var vs = SystemInformation.VirtualScreen;
        int nx = (int)Math.Round((x - vs.X) * 65535.0 / Math.Max(1, vs.Width - 1));
        int ny = (int)Math.Round((y - vs.Y) * 65535.0 / Math.Max(1, vs.Height - 1));
        if (nx < 0) nx = 0; if (nx > 65535) nx = 65535;
        if (ny < 0) ny = 0; if (ny > 65535) ny = 65535;
        Native.Send(new[] { MouseInput(nx, ny, 0, Native.MOUSEEVENTF_MOVE | Native.MOUSEEVENTF_ABSOLUTE | Native.MOUSEEVENTF_VIRTUALDESK) });
    }

    static Native.INPUT MouseInput(int dx, int dy, uint data, uint flags)
    {
        var i = new Native.INPUT();
        i.type = Native.INPUT_MOUSE;
        i.U.mi.dx = dx;
        i.U.mi.dy = dy;
        i.U.mi.mouseData = data;
        i.U.mi.dwFlags = flags;
        return i;
    }

    ActResult Keyboard(string action, Dictionary<string, object> a)
    {
        if (string.IsNullOrEmpty(action)) action = Str(a, "action", "type");
        action = action.ToLowerInvariant();
        string text = Str(a, "text", Str(a, "keys", ""));
        switch (action)
        {
            case "type":
                if (string.IsNullOrEmpty(text)) throw new Exception("`text` is required.");
                AgentCursor.AtCaret("type");
                TypeText(text);
                return ActResult.Say("ok");
            case "key":
            case "press":
                if (string.IsNullOrEmpty(text)) throw new Exception("`text` is a chord such as Return, ctrl+s, alt+Tab, or win.");
                {
                    int hold = Math.Max(0, Math.Min(10000, Int(a, "hold", 0)));
                    var chords = SplitChords(text);
                    for (int i = 0; i < chords.Length; i++)
                    {
                        if (i > 0 && !NoYieldThisCall) UserYield.WaitForIdle(AgentCursor.Current);
                        AgentCursor.AtCursor("key:" + ChordLabel(chords[i]));
                        PressChord(chords[i], hold);
                        if (i + 1 < chords.Length) Thread.Sleep(MotionMode() == "smooth" ? 120 : 30);
                    }
                    return ActResult.Say("ok");
                }
            case "hold":
            case "hold_key":
                {
                    if (string.IsNullOrEmpty(text)) throw new Exception("`text` is required.");
                    double sec = Num(a, "duration", 1);
                    if (sec > 10) sec = 10;
                    if (sec < 0.05) sec = 0.05;
                    HoldChord(text, sec);
                    return ActResult.Say("ok");
                }
            default:
                return ActResult.Fail("Unknown keyboard action `" + action + "`.");
        }
    }

    // "ctrl+shift+t", "ctrl+c, ctrl+v", "ctrl+a Delete", "ctrl + s"
    static string[] SplitChords(string text)
    {
        var list = new List<string>();
        foreach (var seg in text.Split(','))
        {
            string t = Regex.Replace(seg.Trim(), @"\s*\+\s*", "+");
            foreach (var p in t.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries)) list.Add(p);
        }
        return list.ToArray();
    }

    static string ChordLabel(string chord)
    {
        var parts = chord.Split(new[] { '+' }, StringSplitOptions.RemoveEmptyEntries);
        for (int i = 0; i < parts.Length; i++)
        {
            string p = parts[i].Trim().ToLowerInvariant();
            if (p == "control") p = "ctrl";
            if (p == "return") p = "enter";
            if (p == "escape") p = "esc";
            if (p == "super" || p == "meta" || p == "cmd" || p == "windows") p = "win";
            parts[i] = p.Length <= 1 ? p.ToUpperInvariant() : char.ToUpperInvariant(p[0]) + p.Substring(1);
        }
        return string.Join(" + ", parts);
    }

    // smooth: real per-character key events at a visible pace (~25-45 ms, jittered), checking
    // pause-on-user as it goes. fast/instant: bulk chunks of 20 characters.
    static void TypeText(string text)
    {
        bool smooth = MotionMode() == "smooth";
        double baseMs = 35;
        if (smooth && text.Length * baseMs > 12000) baseMs = Math.Max(4, 12000.0 / text.Length);  // long text: keep it under ~12 s
        var list = new List<Native.INPUT>();
        int n = 0;
        if (smooth) Native.timeBeginPeriod(1);
        try
        {
            foreach (char ch in text)
            {
                if (ch == '\r') continue;
                if (ch == '\n') PressVkInto(list, 0x0D, false); // Enter
                else
                {
                    list.Add(KeyInput(0, ch, Native.KEYEVENTF_UNICODE));
                    list.Add(KeyInput(0, ch, Native.KEYEVENTF_UNICODE | Native.KEYEVENTF_KEYUP));
                }
                n++;
                if (smooth)
                {
                    Native.Send(list.ToArray());
                    list.Clear();
                    if (n % 20 == 0) AgentCursor.AtCursor("type");          // keep the pill awake
                    if (!NoYieldThisCall && n % 4 == 0) UserYield.WaitForIdle(AgentCursor.Current);
                    Thread.Sleep((int)Math.Round(baseMs * (0.72 + Rand() * 0.56)));
                }
                else if (list.Count >= 40)
                {
                    Native.Send(list.ToArray());
                    list.Clear();
                    if (!NoYieldThisCall) UserYield.WaitForIdle(AgentCursor.Current); // keys are all released here
                }
            }
            if (list.Count > 0) Native.Send(list.ToArray());
        }
        finally { if (smooth) Native.timeEndPeriod(1); }
    }

    static Native.INPUT KeyInput(ushort vk, ushort scan, uint flags)
    {
        var i = new Native.INPUT();
        i.type = Native.INPUT_KEYBOARD;
        i.U.ki.wVk = vk;
        i.U.ki.wScan = scan;
        i.U.ki.dwFlags = flags;
        return i;
    }

    static void PressVkInto(List<Native.INPUT> list, ushort vk, bool ext)
    {
        uint f = ext ? Native.KEYEVENTF_EXTENDEDKEY : 0;
        list.Add(KeyInput(vk, 0, f));
        list.Add(KeyInput(vk, 0, f | Native.KEYEVENTF_KEYUP));
    }

    // Modifiers down, then the key, then release in reverse order. Everything that went
    // down is always released, even if something throws.
    static void PressChord(string chord, int holdMs)
    {
        ushort[] mods;
        ushort key;
        bool ext;
        ParseChord(chord, out mods, out key, out ext);
        bool smooth = MotionMode() == "smooth";
        int gap = smooth ? 25 : 0;
        var downed = new List<KeyValuePair<ushort, uint>>();
        try
        {
            foreach (var m in mods)
            {
                uint mf = m == 0x5B ? Native.KEYEVENTF_EXTENDEDKEY : 0;
                Native.Send(new[] { KeyInput(m, 0, mf) });
                downed.Add(new KeyValuePair<ushort, uint>(m, mf));
                if (gap > 0) Thread.Sleep(gap);
            }
            uint f = ext ? Native.KEYEVENTF_EXTENDEDKEY : 0;
            Native.Send(new[] { KeyInput(key, 0, f) });
            downed.Add(new KeyValuePair<ushort, uint>(key, f));
            int hold = holdMs > 0 ? holdMs : smooth ? 45 + (int)(Rand() * 25) : 0;
            if (hold > 0) Thread.Sleep(hold);
        }
        finally
        {
            for (int i = downed.Count - 1; i >= 0; i--)
            {
                try { Native.Send(new[] { KeyInput(downed[i].Key, 0, downed[i].Value | Native.KEYEVENTF_KEYUP) }); } catch { }
                if (gap > 0 && i > 0) Thread.Sleep(gap / 2);
            }
        }
        Thread.Sleep(smooth ? 30 : 15);
    }

    static void HoldChord(string chord, double sec)
    {
        AgentCursor.AtCursor("key:" + ChordLabel(chord));
        PressChord(chord, (int)(sec * 1000));
    }

    // reach selftest: offscreen checks of the input logic (nothing is sent to the OS).
    public static string SelfTest()
    {
        var sb = new StringBuilder(); int pass = 0, fail = 0;
        Action<string, bool> ok = (name, cond) => { if (cond) pass++; else { fail++; sb.AppendLine("FAIL " + name); } };
        Func<string, string> split = t => string.Join("|", SplitChords(t));
        ok("seq comma", split("ctrl+c, ctrl+v") == "ctrl+c|ctrl+v");
        ok("spaced chord", split("ctrl + shift + t") == "ctrl+shift+t");
        ok("space seq", split("ctrl+a Delete") == "ctrl+a|Delete");
        ushort[] m; ushort k; bool e;
        ParseChord("ctrl+shift+t", out m, out k, out e);
        ok("ctrl+shift+t", m.Length == 2 && m[0] == 0x11 && m[1] == 0x10 && k == 0x54);
        ParseChord("win+r", out m, out k, out e);
        ok("win+r", m.Length == 1 && m[0] == 0x5B && k == 0x52);
        ParseChord("shift", out m, out k, out e);
        ok("bare shift", m.Length == 0 && k == 0x10);
        ParseChord("alt+F4", out m, out k, out e);
        ok("alt+F4", m.Length == 1 && m[0] == 0x12 && k == 0x73);
        bool blocked = false; try { ParseChord("win+l", out m, out k, out e); } catch { blocked = true; }
        ok("win+l blocked", blocked);
        ok("label", ChordLabel("ctrl+shift+t") == "Ctrl + Shift + T" && ChordLabel("win+r") == "Win + R" && ChordLabel("control+Return") == "Ctrl + Enter");
        // glide: ease endpoints and exact landing for any curve
        var r = new Random(1);
        bool land = true, mono = true, bounds = true;
        for (int i = 0; i < 2000; i++)
        {
            double cx = r.Next(-1920, 1920), cy = r.Next(0, 1080), tx = r.Next(-1920, 1920), ty = r.Next(0, 1080);
            double dx = tx - cx, dy = ty - cy, d = Math.Sqrt(dx * dx + dy * dy); if (d < 3) continue;
            double off = (0.5 + r.NextDouble() * 0.5) * Math.Min(60, d * 0.12) * (r.Next(2) == 0 ? -1 : 1);
            double kx = (cx + tx) / 2.0 - dy / d * off, ky = (cy + ty) / 2.0 + dx / d * off;
            double prev = -1;
            for (double t = 0; t <= 1.0001; t += 1 / 54.0)
            {
                double tt = Math.Min(1, t), ee = tt < 0.5 ? 4 * tt * tt * tt : 1 - Math.Pow(-2 * tt + 2, 3) / 2;
                if (ee < prev - 1e-12) mono = false; prev = ee;
            }
            double u = 0; double ex = u * u * cx + 2 * u * 1 * kx + 1 * tx, ey = u * u * cy + 2 * u * 1 * ky + 1 * ty;
            if ((int)Math.Round(ex) != (int)tx || (int)Math.Round(ey) != (int)ty) land = false;
            double ms = Math.Min(450, 150 + d * 0.22);
            if (ms < 150 || ms > 450) bounds = false;
        }
        ok("ease monotonic", mono); ok("lands exactly", land); ok("duration 150-450 ms", bounds);
        double baseMs = 35; int n = 1000; if (n * baseMs > 12000) baseMs = Math.Max(4, 12000.0 / n);
        ok("long text capped", baseMs * n <= 12000.5);
        FastThisCall = true; string fm = MotionMode(); FastThisCall = false;
        ok("--fast", Config.Current.Motion != "smooth" || fm == "fast");
        sb.Append(pass + " passed, " + fail + " failed");
        return sb.ToString();
    }

    static void ParseChord(string chord, out ushort[] mods, out ushort key, out bool ext)
    {
        var parts = chord.Contains("+") || chord.Length < 3 ? chord.Split(new[] { '+' }, StringSplitOptions.RemoveEmptyEntries)
                                                             : chord.Split(new[] { '+', '-' }, StringSplitOptions.RemoveEmptyEntries);
        var ml = new List<ushort>();
        key = 0;
        ext = false;
        for (int i = 0; i < parts.Length; i++)
        {
            string p = parts[i].Trim().ToLowerInvariant();
            ushort vk;
            bool e;
            bool isMod = p == "ctrl" || p == "control" || p == "alt" || p == "shift"
                      || p == "win" || p == "super" || p == "meta" || p == "cmd" || p == "windows";
            if (isMod && i < parts.Length - 1)
            {
                if (p == "ctrl" || p == "control") ml.Add(0x11);
                else if (p == "alt") ml.Add(0x12);
                else if (p == "shift") ml.Add(0x10);
                else ml.Add(0x5B); // LWIN
            }
            else
            {
                if (!LookupKey(p, out vk, out e))
                    throw new Exception("Unknown key `" + parts[i] + "`.");
                key = vk;
                ext = e;
            }
        }
        if (key == 0) throw new Exception("No key in chord `" + chord + "`.");
        // Block Win+L (lock the PC): Reach should not lock the machine.
        bool hasWin = ml.Contains(0x5B);
        if (hasWin && (key == 0x4C /*L*/))
            throw new Exception("Win+L is blocked so Reach cannot lock the PC.");
        mods = ml.ToArray();
    }

    static bool LookupKey(string p, out ushort vk, out bool ext)
    {
        ext = false;
        vk = 0;
        p = p.ToLowerInvariant();
        if (p.Length == 1)
        {
            char c = p[0];
            if (c >= 'a' && c <= 'z') { vk = (ushort)(c - 'a' + 0x41); return true; }
            if (c >= '0' && c <= '9') { vk = (ushort)c; return true; }
        }
        switch (p)
        {
            case "return": case "enter": vk = 0x0D; return true;
            case "tab": vk = 0x09; return true;
            case "esc": case "escape": vk = 0x1B; return true;
            case "space": case "spacebar": vk = 0x20; return true;
            case "backspace": case "bksp": vk = 0x08; return true;
            case "delete": case "del": vk = 0x2E; ext = true; return true;
            case "insert": case "ins": vk = 0x2D; ext = true; return true;
            case "home": vk = 0x24; ext = true; return true;
            case "end": vk = 0x23; ext = true; return true;
            case "pageup": case "pgup": vk = 0x21; ext = true; return true;
            case "pagedown": case "pgdn": vk = 0x22; ext = true; return true;
            case "up": case "uparrow": vk = 0x26; ext = true; return true;
            case "down": case "downarrow": vk = 0x28; ext = true; return true;
            case "left": case "leftarrow": vk = 0x25; ext = true; return true;
            case "right": case "rightarrow": vk = 0x27; ext = true; return true;
            case "win": case "super": case "meta": case "cmd": vk = 0x5B; return true;
            case "apps": case "menu": vk = 0x5D; return true;
            case "ctrl": case "control": vk = 0x11; return true;
            case "alt": vk = 0x12; return true;
            case "shift": vk = 0x10; return true;
            case "capslock": vk = 0x14; return true;
            case "printscreen": case "prtsc": vk = 0x2C; return true;
        }
        int fn;
        if (p.Length >= 2 && (p[0] == 'f' || p[0] == 'F') && int.TryParse(p.Substring(1), out fn) && fn >= 1 && fn <= 24)
        {
            vk = (ushort)(0x70 + fn - 1);
            return true;
        }
        return false;
    }

    ActResult Apps(string action, Dictionary<string, object> a)
    {
        if (string.IsNullOrEmpty(action)) action = Str(a, "action", "list");
        action = action.ToLowerInvariant();
        switch (action)
        {
            case "list":
                return ActResult.Say(ListWindows(Str(a, "filter", Str(a, "query", "")), Bool(a, "json", false)));
            case "open":
            case "launch":
            case "start":
                {
                    string name = Str(a, "name", Str(a, "app", Str(a, "text", Str(a, "query", ""))));
                    if (string.IsNullOrEmpty(name)) throw new Exception("`name` is required (for example notepad, chrome).");
                    bool fresh = Bool(a, "new", false);
                    return OpenApp(name, fresh);
                }
            case "focus":
                return FocusWindow(ParseHwndOrName(a), true);
            case "close":
                {
                    IntPtr h = ParseHwndOrName(a);
                    Native.PostMessage(h, 0x0010 /*WM_CLOSE*/, IntPtr.Zero, IntPtr.Zero);
                    return ActResult.Say("close sent to " + FmtHwnd(h));
                }
            case "minimize":
                Native.ShowWindow(ParseHwndOrName(a), Native.SW_SHOWMINIMIZED);
                return ActResult.Say("ok");
            case "maximize":
                Native.ShowWindow(ParseHwndOrName(a), Native.SW_SHOWMAXIMIZED);
                return ActResult.Say("ok");
            case "restore":
                Native.ShowWindow(ParseHwndOrName(a), Native.SW_RESTORE);
                return ActResult.Say("ok");
            case "displays":
                return ActResult.Say(ListDisplays());
            default:
                return ActResult.Fail("Unknown apps action `" + action + "`. Use list, open, focus, close, minimize, maximize, restore, displays.");
        }
    }

    string ListDisplays()
    {
        var map = ScreenMap.Build(Config.Current.MaxDim);
        var sb = new StringBuilder();
        int i = 0;
        foreach (var sc in Screen.AllScreens)
        {
            var tl = map.ToImage(sc.Bounds.X, sc.Bounds.Y);
            var br = map.ToImage(sc.Bounds.Right, sc.Bounds.Bottom);
            sb.Append("d").Append(++i).Append(" ")
              .Append(tl.X).Append(",").Append(tl.Y).Append(" ")
              .Append(br.X - tl.X).Append("x").Append(br.Y - tl.Y)
              .Append(" work ");
            var wtl = map.ToImage(sc.WorkingArea.X, sc.WorkingArea.Y);
            var wbr = map.ToImage(sc.WorkingArea.Right, sc.WorkingArea.Bottom);
            sb.Append(wtl.X).Append(",").Append(wtl.Y).Append(" ")
              .Append(wbr.X - wtl.X).Append("x").Append(wbr.Y - wtl.Y);
            if (sc.Primary) sb.Append(" primary");
            sb.AppendLine();
        }
        sb.Append("image ").Append(map.OutW).Append('x').Append(map.OutH);
        return sb.ToString().TrimEnd();
    }

    class WinInfo
    {
        public IntPtr H;
        public string Title, Process, Path;
        public Rectangle Bounds;
        public bool Focused, Minimized, Maximized;
        public int Pid;
    }

    List<WinInfo> EnumerateWindows()
    {
        var list = new List<WinInfo>();
        IntPtr fg = Native.GetForegroundWindow();
        Native.EnumWindows(delegate(IntPtr h, IntPtr l)
        {
            if (!Native.IsWindowVisible(h)) return true;
            int ex = Native.GetWindowLong(h, Native.GWL_EXSTYLE);
            if ((ex & Native.WS_EX_TOOLWINDOW) != 0) return true;
            if ((ex & Native.WS_EX_NOACTIVATE) != 0) return true;
            string title = Native.TitleOf(h);
            // Owner-less top-level (or with title)
            uint pid;
            Native.GetWindowThreadProcessId(h, out pid);
            if (pid == 0) return true;
            string proc = "", path = "";
            try
            {
                var p = Process.GetProcessById((int)pid);
                proc = p.ProcessName;
                try { path = p.MainModule.FileName; } catch { }
            }
            catch { }
            if (string.IsNullOrWhiteSpace(title) && string.IsNullOrEmpty(proc)) return true;
            // Skip untitled tool-like windows with no size
            Native.RECT r;
            if (!Native.GetWindowRect(h, out r)) return true;
            var bounds = Rectangle.FromLTRB(r.Left, r.Top, r.Right, r.Bottom);
            if (bounds.Width < 10 || bounds.Height < 10) return true;
            list.Add(new WinInfo
            {
                H = h,
                Title = title,
                Process = proc,
                Path = path ?? "",
                Bounds = bounds,
                Focused = (h == fg),
                Minimized = Native.IsIconic(h),
                Maximized = Native.IsZoomed(h),
                Pid = (int)pid
            });
            return true;
        }, IntPtr.Zero);
        return list;
    }

    string ListWindows(string filter, bool json)
    {
        var map = ScreenMap.Build(Config.Current.MaxDim);
        var sb = new StringBuilder();
        var list = new List<object>();
        string f = string.IsNullOrEmpty(filter) ? null : filter.ToLowerInvariant();
        foreach (var w in EnumerateWindows())
        {
            if (f != null && (w.Title ?? "").ToLowerInvariant().IndexOf(f) < 0
                && (w.Process ?? "").ToLowerInvariant().IndexOf(f) < 0) continue;
            var tl = map.ToImage(w.Bounds.X, w.Bounds.Y);
            var br = map.ToImage(w.Bounds.Right, w.Bounds.Bottom);
            int ww = Math.Max(0, br.X - tl.X), hh = Math.Max(0, br.Y - tl.Y);
            if (json)
            {
                var d = new Dictionary<string, object>();
                d["hwnd"] = FmtHwnd(w.H); d["title"] = w.Title ?? ""; d["process"] = w.Process ?? ""; d["pid"] = w.Pid;
                d["x"] = tl.X; d["y"] = tl.Y; d["w"] = ww; d["h"] = hh;
                d["focused"] = w.Focused; d["minimized"] = w.Minimized; d["maximized"] = w.Maximized;
                list.Add(d);
                continue;
            }
            string title = string.IsNullOrEmpty(w.Title) ? "-" : Clip(w.Title, 70);
            sb.Append(FmtHwnd(w.H)).Append(' ').Append(title);
            if (!string.Equals(w.Process, w.Title, StringComparison.OrdinalIgnoreCase)) sb.Append(" [").Append(w.Process).Append(']');
            sb.Append(' ').Append(tl.X).Append(',').Append(tl.Y).Append(' ').Append(ww).Append('x').Append(hh);
            if (w.Focused) sb.Append(" focused");
            if (w.Minimized) sb.Append(" min");
            if (w.Maximized) sb.Append(" max");
            sb.Append('\n');
        }
        if (json) return new JavaScriptSerializer().Serialize(list);
        if (sb.Length == 0) return f == null ? "No open windows." : "No window matched `" + filter + "`.";
        return sb.ToString().TrimEnd();
    }

    ActResult FocusWindow(IntPtr h, bool report)
    {
        Native.ForceFocus(h);
        return ActResult.Say("focused " + FmtHwnd(h) + " " + Native.TitleOf(h));
    }

    IntPtr ParseHwndOrName(Dictionary<string, object> a)
    {
        string w = Str(a, "window", Str(a, "hwnd", Str(a, "name", Str(a, "app", Str(a, "text", "")))));
        if (string.IsNullOrEmpty(w)) throw new Exception("`window` (handle or name) is required.");
        if (LooksLikeHwnd(w)) return ParseHwnd(w);
        var hits = new List<WinInfo>();
        string f = w.ToLowerInvariant();
        foreach (var info in EnumerateWindows())
        {
            if ((info.Title ?? "").ToLowerInvariant().IndexOf(f) >= 0
                || (info.Process ?? "").ToLowerInvariant() == f
                || (info.Process ?? "").ToLowerInvariant().IndexOf(f) >= 0)
                hits.Add(info);
        }
        if (hits.Count == 0) throw new Exception("No open window matching `" + w + "`.");
        // Prefer exact process, then focused, then first
        foreach (var h in hits)
            if (string.Equals(h.Process, w, StringComparison.OrdinalIgnoreCase)) return h.H;
        foreach (var h in hits)
            if (h.Focused) return h.H;
        return hits[0].H;
    }

    ActResult OpenApp(string name, bool fresh)
    {
        name = name.Trim();
        if (!fresh)
        {
            // If it's already open, just focus it: that's what you usually want.
            var hits = new List<WinInfo>();
            string f = name.ToLowerInvariant();
            foreach (var info in EnumerateWindows())
            {
                if ((info.Title ?? "").ToLowerInvariant().IndexOf(f) >= 0
                    || (info.Process ?? "").ToLowerInvariant() == f
                    || (info.Process ?? "").ToLowerInvariant().Replace(" ", "") == f.Replace(" ", ""))
                    hits.Add(info);
            }
            if (hits.Count > 0)
            {
                var pick = hits[0];
                foreach (var h in hits)
                    if (string.Equals(h.Process, name, StringComparison.OrdinalIgnoreCase)) { pick = h; break; }
                Native.ForceFocus(pick.H);
                return ActResult.Say("already open, focused " + FmtHwnd(pick.H) + "  " + pick.Title + "  [" + pick.Process + "]");
            }
        }

        // Direct path or URL
        if (name.StartsWith("http://", StringComparison.OrdinalIgnoreCase)
            || name.StartsWith("https://", StringComparison.OrdinalIgnoreCase)
            || name.StartsWith("file:", StringComparison.OrdinalIgnoreCase)
            || File.Exists(name) || Directory.Exists(name))
        {
            Launch(name, null);
            return WaitForNew(name);
        }

        string lnk = FindShortcut(name);
        if (lnk != null)
        {
            Launch(lnk, null);
            return WaitForNew(name);
        }

        string appPath = FindAppPath(name);
        if (appPath != null)
        {
            Launch(appPath, null);
            return WaitForNew(name);
        }

        // Shell execute the name itself (notepad, calc, ms-settings:, etc.)
        try
        {
            Launch(name, null);
            return WaitForNew(name);
        }
        catch (Exception ex)
        {
            return ActResult.Fail("Could not open `" + name + "`. " + ex.Message);
        }
    }

    ActResult WaitForNew(string name)
    {
        string f = name.ToLowerInvariant();
        // strip path / extension for matching
        try { f = Path.GetFileNameWithoutExtension(name).ToLowerInvariant(); } catch { }
        for (int i = 0; i < 12; i++)
        {
            Thread.Sleep(250);
            foreach (var info in EnumerateWindows())
            {
                if ((info.Title ?? "").ToLowerInvariant().IndexOf(f) >= 0
                    || (info.Process ?? "").ToLowerInvariant().IndexOf(f) >= 0)
                {
                    Native.ForceFocus(info.H);
                    return ActResult.Say("opened " + FmtHwnd(info.H) + "  " + info.Title + "  [" + info.Process + "]");
                }
            }
        }
        return ActResult.Say("launched `" + name + "`, but no window showed up yet. Run `reach apps` to check.");
    }

    static void Launch(string file, string args)
    {
        var psi = new ProcessStartInfo();
        psi.FileName = file;
        if (!string.IsNullOrEmpty(args)) psi.Arguments = args;
        psi.UseShellExecute = true;
        psi.WorkingDirectory = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        Process.Start(psi);
    }

    static List<string> lnkCache;
    static DateTime lnkCacheAt;

    static string FindShortcut(string name)
    {
        string needle = name.ToLowerInvariant();
        if (lnkCache == null || (DateTime.Now - lnkCacheAt).TotalSeconds > 120) { lnkCache = ScanShortcuts(); lnkCacheAt = DateTime.Now; }
        string exact = null, starts = null, contains = null;
        foreach (var f in lnkCache)
        {
            string n = Path.GetFileNameWithoutExtension(f).ToLowerInvariant();
            if (n == needle) { exact = f; break; }
            if (starts == null && n.StartsWith(needle)) starts = f;
            if (contains == null && n.IndexOf(needle) >= 0) contains = f;
        }
        return exact ?? starts ?? contains;
    }

    static List<string> ScanShortcuts()
    {
        var dirs = new List<string>();
        dirs.Add(Environment.GetFolderPath(Environment.SpecialFolder.StartMenu));
        dirs.Add(Environment.GetFolderPath(Environment.SpecialFolder.CommonStartMenu));
        dirs.Add(Environment.GetFolderPath(Environment.SpecialFolder.Desktop));
        dirs.Add(Environment.GetFolderPath(Environment.SpecialFolder.CommonDesktopDirectory));
        dirs.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            @"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"));
        var all = new List<string>();
        foreach (var dir in dirs)
        {
            if (string.IsNullOrEmpty(dir) || !Directory.Exists(dir)) continue;
            try { all.AddRange(Directory.GetFiles(dir, "*.lnk", SearchOption.AllDirectories)); } catch { }
        }
        return all;
    }

    static string FindAppPath(string name)
    {
        try
        {
            string key = @"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\" + name;
            if (!name.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)) key += ".exe";
            var hklm = Microsoft.Win32.Registry.LocalMachine.OpenSubKey(key);
            if (hklm != null)
            {
                object v = hklm.GetValue(null);
                hklm.Close();
                if (v != null && File.Exists(v.ToString().Trim('"'))) return v.ToString().Trim('"');
            }
            var hkcu = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(key);
            if (hkcu != null)
            {
                object v = hkcu.GetValue(null);
                hkcu.Close();
                if (v != null && File.Exists(v.ToString().Trim('"'))) return v.ToString().Trim('"');
            }
        }
        catch { }
        return null;
    }

    ActResult Ui(string action, Dictionary<string, object> a)
    {
        if (string.IsNullOrEmpty(action)) action = Str(a, "action", "inspect");
        action = action.ToLowerInvariant();
        switch (action)
        {
            case "inspect":
                return ActResult.Say(Inspect(a));
            case "find":
                {
                    string q = Str(a, "query", Str(a, "name", Str(a, "text", "")));
                    if (string.IsNullOrEmpty(q)) throw new Exception("`query` is required.");
                    return ActResult.Say(Find(q, a));
                }
            case "invoke":
            case "click":
                return Invoke(Str(a, "ref", ""), a);
            case "set":
            case "set_value":
                return SetValue(Str(a, "ref", ""), Str(a, "value", Str(a, "text", "")), a);
            case "read":
                return ActResult.Say(ReadEl(GetEl(Str(a, "ref", ""), a)));
            case "toggle":
                return Toggle(GetEl(Str(a, "ref", ""), a));
            case "focus":
                {
                    var el = GetEl(Str(a, "ref", ""), a);
                    try { el.SetFocus(); } catch { }
                    return ActResult.Say("ok");
                }
            default:
                return ActResult.Fail("Unknown ui action `" + action + "`. Use inspect, find, invoke, set, read, toggle, focus.");
        }
    }

    AutomationElement RootFor(Dictionary<string, object> a)
    {
        string w = Str(a, "window", "");
        if (!string.IsNullOrEmpty(w))
        {
            IntPtr h = LooksLikeHwnd(w) ? ParseHwnd(w) : ParseHwndOrName(a);
            var el = AutomationElement.FromHandle(h);
            if (el == null) throw new Exception("No automation tree for that window.");
            return el;
        }
        var fg = Native.GetForegroundWindow();
        if (fg != IntPtr.Zero)
        {
            try
            {
                var el = AutomationElement.FromHandle(fg);
                if (el != null) return el;
            }
            catch { }
        }
        return AutomationElement.RootElement;
    }

    AutomationElement GetEl(string id, Dictionary<string, object> a)
    {
        if (string.IsNullOrEmpty(id)) id = Str(a, "ref", "");
        if (string.IsNullOrEmpty(id)) throw new Exception("`ref` is required (eN from ui find, or a control name).");
        if (ElementStore.IsRef(id)) return Elements.Get(id);
        // Not a ref: treat it as a name and resolve it fresh (best interactive match).
        var hits = Collect(RootFor(a), id, false, 1);
        if (hits.Count == 0) throw new Exception("No control matching `" + id + "`.");
        Elements.Add(hits[0].El);
        return hits[0].El;
    }

    // ---- UI Automation: one cached bulk fetch per call instead of a property-by-property walk ----
    sealed class UiItem
    {
        public AutomationElement El;
        public string Type, Name, Value, Aid;
        public System.Windows.Rect Rect;
        public bool Enabled, Focus, Interactive, Offscreen;
        public string Toggle;
        public int Score;
    }

    static CacheRequest uiCache;
    static CacheRequest UiCache()
    {
        if (uiCache != null) return uiCache;
        var cr = new CacheRequest();
        cr.TreeScope = TreeScope.Element;
        cr.TreeFilter = Automation.ControlViewCondition;
        cr.AutomationElementMode = AutomationElementMode.Full;
        cr.Add(AutomationElement.NameProperty);
        cr.Add(AutomationElement.ControlTypeProperty);
        cr.Add(AutomationElement.IsOffscreenProperty);
        cr.Add(AutomationElement.BoundingRectangleProperty);
        cr.Add(AutomationElement.IsEnabledProperty);
        cr.Add(AutomationElement.AutomationIdProperty);
        cr.Add(AutomationElement.HasKeyboardFocusProperty);
        cr.Add(AutomationElement.IsKeyboardFocusableProperty);
        cr.Add(AutomationElement.IsInvokePatternAvailableProperty);
        cr.Add(AutomationElement.IsValuePatternAvailableProperty);
        cr.Add(AutomationElement.IsTogglePatternAvailableProperty);
        cr.Add(AutomationElement.IsSelectionItemPatternAvailableProperty);
        cr.Add(AutomationElement.IsExpandCollapsePatternAvailableProperty);
        cr.Add(ValuePattern.ValueProperty);
        cr.Add(TogglePattern.ToggleStateProperty);
        uiCache = cr;
        return cr;
    }

    static readonly HashSet<string> InteractiveTypes = new HashSet<string> {
        "button", "checkbox", "combobox", "edit", "hyperlink", "listitem", "menuitem", "radiobutton",
        "slider", "spinner", "tabitem", "treeitem", "splitbutton", "dataitem", "headeritem", "document", "menubar"
    };

    static readonly HashSet<string> UnnamedOk = new HashSet<string> { "edit", "document", "combobox", "checkbox", "radiobutton", "slider", "spinner" };
    static readonly HashSet<string> NoiseTypes = new HashSet<string> { "pane", "window", "titlebar", "scrollbar", "thumb", "separator" };

    static object Cached(AutomationElement el, AutomationProperty p)
    {
        try
        {
            object v = el.GetCachedPropertyValue(p, true);
            return v == AutomationElement.NotSupported ? null : v;
        }
        catch { return null; }
    }

    static bool CachedBool(AutomationElement el, AutomationProperty p)
    {
        object v = Cached(el, p);
        return v is bool && (bool)v;
    }

    static UiItem ToItem(AutomationElement el)
    {
        var it = new UiItem { El = el };
        var ct = Cached(el, AutomationElement.ControlTypeProperty) as ControlType;
        it.Type = ct == null ? "?" : ct.ProgrammaticName.Replace("ControlType.", "").ToLowerInvariant();
        it.Name = Cached(el, AutomationElement.NameProperty) as string ?? "";
        it.Value = Cached(el, ValuePattern.ValueProperty) as string ?? "";
        it.Aid = Cached(el, AutomationElement.AutomationIdProperty) as string ?? "";
        object r = Cached(el, AutomationElement.BoundingRectangleProperty);
        it.Rect = r is System.Windows.Rect ? (System.Windows.Rect)r : System.Windows.Rect.Empty;
        it.Offscreen = CachedBool(el, AutomationElement.IsOffscreenProperty);
        it.Enabled = CachedBool(el, AutomationElement.IsEnabledProperty);
        it.Focus = CachedBool(el, AutomationElement.HasKeyboardFocusProperty);
        bool pat = CachedBool(el, AutomationElement.IsInvokePatternAvailableProperty)
                || CachedBool(el, AutomationElement.IsTogglePatternAvailableProperty)
                || CachedBool(el, AutomationElement.IsSelectionItemPatternAvailableProperty)
                || CachedBool(el, AutomationElement.IsExpandCollapsePatternAvailableProperty)
                || CachedBool(el, AutomationElement.IsValuePatternAvailableProperty);
        it.Interactive = InteractiveTypes.Contains(it.Type) || pat;
        if (CachedBool(el, AutomationElement.IsTogglePatternAvailableProperty))
        {
            object ts = Cached(el, TogglePattern.ToggleStateProperty);
            if (ts is ToggleState) it.Toggle = (ToggleState)ts == ToggleState.On ? "on" : (ToggleState)ts == ToggleState.Off ? "off" : "mixed";
        }
        return it;
    }

    // Useful = visible, has size, and is either interactive or carries a name.
    static bool Useful(UiItem it)
    {
        if (it.Offscreen || it.Rect.IsEmpty || it.Rect.Width < 1 || it.Rect.Height < 1) return false;
        if (it.Interactive) return it.Name.Trim().Length > 0 || it.Value.Length > 0 || UnnamedOk.Contains(it.Type);
        if (it.Name.Trim().Length == 0) return false;
        return !NoiseTypes.Contains(it.Type);
    }

    // query == null: inspect. all: no filtering (raw, like a plain tree dump).
    List<UiItem> Collect(AutomationElement root, string query, bool all, int limit)
    {
        AutomationElementCollection col;
        using (UiCache().Activate())
            col = root.FindAll(TreeScope.Subtree, Automation.ControlViewCondition);
        var outList = new List<UiItem>();
        string q = query == null ? null : query.ToLowerInvariant();
        UiItem lastKept = null;
        foreach (AutomationElement el in col)
        {
            UiItem it;
            try { it = ToItem(el); } catch { continue; }
            if (all) { if (!it.Offscreen) outList.Add(it); continue; }
            if (q != null)
            {
                if (it.Offscreen) continue;
                string n = it.Name.ToLowerInvariant();
                bool hit = n.IndexOf(q) >= 0 || it.Value.ToLowerInvariant().IndexOf(q) >= 0
                        || it.Aid.ToLowerInvariant() == q || it.Type == q;
                if (!hit) continue;
                it.Score = (n == q ? 0 : n.StartsWith(q) ? 1 : 2) * 2 + (it.Interactive ? 0 : 1);
                outList.Add(it);
                continue;
            }
            if (!Useful(it)) continue;
            // Chromium/WPF often repeat a button's name in a text child; skip that echo.
            if (lastKept != null && !it.Interactive && it.Type == "text" && it.Name == lastKept.Name
                && lastKept.Rect.Contains(it.Rect)) continue;
            outList.Add(it);
            lastKept = it;
        }
        if (q != null)
        {
            // stable sort by score
            var idx = new Dictionary<UiItem, int>();
            for (int i = 0; i < outList.Count; i++) idx[outList[i]] = i;
            outList.Sort((x, y) => x.Score != y.Score ? x.Score.CompareTo(y.Score) : idx[x].CompareTo(idx[y]));
        }
        return outList;
    }

    static CacheRequest uiCacheKids;

    // Inspect: depth-first walk, one cached round trip per node (props + children),
    // stopping as soon as `limit` useful controls are found.
    List<UiItem> CollectWalk(AutomationElement root, bool all, int limit, int maxDepth)
    {
        if (uiCacheKids == null)
        {
            var c = UiCache().Clone();
            c.TreeScope = TreeScope.Element | TreeScope.Children;
            uiCacheKids = c;
        }
        var outList = new List<UiItem>();
        var stack = new Stack<KeyValuePair<AutomationElement, int>>();
        stack.Push(new KeyValuePair<AutomationElement, int>(root, 0));
        UiItem lastKept = null;
        using (uiCacheKids.Activate())
        {
            while (stack.Count > 0 && outList.Count <= limit)
            {
                var top = stack.Pop();
                AutomationElement el;
                try { el = top.Key.GetUpdatedCache(uiCacheKids); } catch { continue; }
                UiItem it;
                try { it = ToItem(el); } catch { continue; }
                bool keep = all ? !it.Offscreen : Useful(it);
                if (keep && !all && lastKept != null && !it.Interactive && it.Type == "text"
                    && it.Name == lastKept.Name && lastKept.Rect.Contains(it.Rect)) keep = false;
                if (keep) { outList.Add(it); lastKept = it; }
                if (top.Value >= maxDepth) continue;
                AutomationElementCollection kids;
                try { kids = el.CachedChildren; } catch { continue; }
                if (kids == null) continue;
                for (int i = kids.Count - 1; i >= 0; i--)
                    stack.Push(new KeyValuePair<AutomationElement, int>(kids[i], top.Value + 1));
            }
        }
        return outList;
    }

    string Inspect(Dictionary<string, object> a)
    {
        int limit = Int(a, "limit", 60);
        var items = CollectWalk(RootFor(a), Bool(a, "all", false), limit, Int(a, "depth", 25));
        string r = FormatItems(items, limit, Bool(a, "json", false), "No controls found.");
        return r.Replace("+1 more (--limit)", "+more (--limit)");
    }

    string Find(string query, Dictionary<string, object> a)
    {
        int limit = Int(a, "limit", 25);
        return FormatItems(Collect(RootFor(a), query, Bool(a, "all", false), limit), limit, Bool(a, "json", false),
            "No control matching `" + query + "`.");
    }

    string FormatItems(List<UiItem> items, int limit, bool json, string empty)
    {
        if (items.Count == 0) return json ? "[]" : empty;
        var map = ScreenMap.Build(Config.Current.MaxDim);
        int shown = Math.Min(limit, items.Count);
        if (json)
        {
            var list = new List<object>();
            for (int i = 0; i < shown; i++)
            {
                var it = items[i];
                var tl = map.ToImage((int)it.Rect.X, (int)it.Rect.Y);
                var br = map.ToImage((int)it.Rect.Right, (int)it.Rect.Bottom);
                var d = new Dictionary<string, object>();
                d["ref"] = Elements.Add(it.El);
                d["type"] = it.Type;
                d["name"] = it.Name;
                if (it.Value.Length > 0) d["value"] = it.Value;
                if (it.Aid.Length > 0) d["aid"] = it.Aid;
                d["x"] = tl.X; d["y"] = tl.Y; d["w"] = Math.Max(0, br.X - tl.X); d["h"] = Math.Max(0, br.Y - tl.Y);
                d["cx"] = (tl.X + br.X) / 2; d["cy"] = (tl.Y + br.Y) / 2;
                d["enabled"] = it.Enabled;
                if (it.Focus) d["focused"] = true;
                if (it.Toggle != null) d["toggle"] = it.Toggle;
                d["interactive"] = it.Interactive;
                list.Add(d);
            }
            var o = new Dictionary<string, object>();
            o["items"] = list;
            o["total"] = items.Count;
            return new JavaScriptSerializer { MaxJsonLength = 8 * 1024 * 1024 }.Serialize(o);
        }
        var sb = new StringBuilder();
        for (int i = 0; i < shown; i++)
        {
            var it = items[i];
            var tl = map.ToImage((int)it.Rect.X, (int)it.Rect.Y);
            var br = map.ToImage((int)it.Rect.Right, (int)it.Rect.Bottom);
            sb.Append(Elements.Add(it.El)).Append(' ').Append(it.Type);
            if (it.Name.Length > 0) sb.Append(' ').Append(Quote(Clip(it.Name, 60)));
            if (it.Value.Length > 0 && it.Value != it.Name) sb.Append(" =").Append(Quote(Clip(it.Value, 40)));
            sb.Append(' ').Append((tl.X + br.X) / 2).Append(',').Append((tl.Y + br.Y) / 2);
            if (it.Toggle != null) sb.Append(' ').Append(it.Toggle);
            if (!it.Enabled) sb.Append(" disabled");
            if (it.Focus) sb.Append(" focused");
            sb.Append('\n');
        }
        if (items.Count > shown) sb.Append("+").Append(items.Count - shown).Append(" more (--limit)");
        return sb.ToString().TrimEnd();
    }

    static string Clip(string s, int n)
    {
        s = (s ?? "").Replace("\r", " ").Replace("\n", " ").Trim();
        return s.Length > n ? s.Substring(0, n - 1) + "…" : s;
    }

    static void PointAt(AutomationElement el, string kind)
    {
        try
        {
            var r = el.Current.BoundingRectangle;
            if (!r.IsEmpty) AgentCursor.At(new Point((int)(r.X + r.Width / 2), (int)(r.Y + r.Height / 2)), kind);
        }
        catch { }
    }

    ActResult Invoke(string id, Dictionary<string, object> a)
    {
        var el = GetEl(id, a);
        PointAt(el, "click");
        try { el.SetFocus(); } catch { }
        object pat;
        if (el.TryGetCurrentPattern(InvokePattern.Pattern, out pat))
        {
            ((InvokePattern)pat).Invoke();
            return ActResult.Say("invoked " + id);
        }
        if (el.TryGetCurrentPattern(TogglePattern.Pattern, out pat))
        {
            ((TogglePattern)pat).Toggle();
            return ActResult.Say("toggled " + id);
        }
        if (el.TryGetCurrentPattern(SelectionItemPattern.Pattern, out pat))
        {
            ((SelectionItemPattern)pat).Select();
            return ActResult.Say("selected " + id);
        }
        if (el.TryGetCurrentPattern(ExpandCollapsePattern.Pattern, out pat))
        {
            var e = (ExpandCollapsePattern)pat;
            if (e.Current.ExpandCollapseState == ExpandCollapseState.Expanded) e.Collapse();
            else e.Expand();
            return ActResult.Say("expanded/collapsed " + id);
        }
        // Fallback: click its center in image space via mouse
        try
        {
            var r = el.Current.BoundingRectangle;
            if (!r.IsEmpty)
            {
                var map = ScreenMap.Build(Config.Current.MaxDim);
                var mid = map.ToImage((int)(r.X + r.Width / 2), (int)(r.Y + r.Height / 2));
                Click(map, mid.X, mid.Y, "left", 1);
                return ActResult.Say("clicked " + id + " (no invoke pattern)");
            }
        }
        catch { }
        return ActResult.Fail("Could not invoke " + id + ".");
    }

    ActResult SetValue(string id, string value, Dictionary<string, object> a)
    {
        var el = GetEl(id, a);
        PointAt(el, "type");
        object pat;
        if (el.TryGetCurrentPattern(ValuePattern.Pattern, out pat))
        {
            ((ValuePattern)pat).SetValue(value ?? "");
            return ActResult.Say("set " + id);
        }
        try { el.SetFocus(); } catch { }
        Thread.Sleep(50);
        // Select-all then type
        PressChord("ctrl+a", 0);
        Thread.Sleep(30);
        TypeText(value ?? "");
        return ActResult.Say("typed into " + id);
    }

    ActResult Toggle(AutomationElement el)
    {
        object pat;
        if (el.TryGetCurrentPattern(TogglePattern.Pattern, out pat))
        {
            ((TogglePattern)pat).Toggle();
            return ActResult.Say("toggled");
        }
        return ActResult.Fail("That control has no toggle pattern.");
    }

    string ReadEl(AutomationElement el)
    {
        var sb = new StringBuilder();
        try
        {
            sb.Append("[").Append(el.Current.ControlType.LocalizedControlType).Append("] ");
            sb.Append(Quote(el.Current.Name ?? ""));
        }
        catch { }
        try
        {
            object pat;
            if (el.TryGetCurrentPattern(ValuePattern.Pattern, out pat))
                sb.Append("  value=").Append(Quote(((ValuePattern)pat).Current.Value));
            if (el.TryGetCurrentPattern(TextPattern.Pattern, out pat))
            {
                var t = ((TextPattern)pat).DocumentRange.GetText(2000);
                if (!string.IsNullOrEmpty(t)) sb.Append("  text=").Append(Quote(t));
            }
            if (el.TryGetCurrentPattern(TogglePattern.Pattern, out pat))
                sb.Append("  toggle=").Append(((TogglePattern)pat).Current.ToggleState);
        }
        catch { }
        return sb.ToString().Trim();
    }

    // ---------- helpers ----------
    static void NeedCoord(int[] c)
    {
        if (c == null || c.Length != 2) throw new Exception("`coordinate` [x,y] (or x and y) is required.");
    }

    static bool LooksLikeHwnd(string s)
    {
        if (string.IsNullOrEmpty(s)) return false;
        if (s.StartsWith("0x") || s.StartsWith("0X")) return true;
        long n;
        return long.TryParse(s, out n) && n > 0 && (s.Length >= 5 || n > 0x10000);
    }

    static IntPtr ParseHwnd(string s)
    {
        if (string.IsNullOrEmpty(s)) throw new Exception("Window handle is required.");
        s = s.Trim();
        try
        {
            long v = s.StartsWith("0x", StringComparison.OrdinalIgnoreCase)
                ? Convert.ToInt64(s, 16)
                : Convert.ToInt64(s);
            var h = new IntPtr(v);
            if (!Native.IsWindow(h)) throw new Exception("Not a live window: " + s);
            return h;
        }
        catch (Exception ex)
        {
            if (ex.Message.StartsWith("Not a live")) throw;
            throw new Exception("Bad window handle `" + s + "`.");
        }
    }

    static string FmtHwnd(IntPtr h)
    {
        return "0x" + h.ToInt64().ToString("X");
    }

    static string Quote(string s)
    {
        if (s == null) return "\"\"";
        s = s.Replace("\r", " ").Replace("\n", " ");
        if (s.Length > 80) s = s.Substring(0, 77) + "...";
        return "\"" + s.Replace("\"", "'") + "\"";
    }

    static string Short(string s)
    {
        if (s == null) return "";
        s = s.Replace("\r", " ").Replace("\n", " ");
        return s.Length > 80 ? s.Substring(0, 77) + "..." : s;
    }

    static string Str(Dictionary<string, object> a, string k, string def)
    {
        object v;
        if (a == null || !a.TryGetValue(k, out v) || v == null) return def;
        return Convert.ToString(v) ?? def;
    }

    static int Int(Dictionary<string, object> a, string k, int def)
    {
        object v;
        if (a == null || !a.TryGetValue(k, out v) || v == null || v is DBNull) return def;
        try { return Convert.ToInt32(v); } catch { return def; }
    }

    static bool TryInt(Dictionary<string, object> a, string k, out int v)
    {
        v = 0;
        object o;
        if (a == null || !a.TryGetValue(k, out o) || o == null) return false;
        try { v = Convert.ToInt32(o); return true; } catch { return false; }
    }

    static double Num(Dictionary<string, object> a, string k, double def)
    {
        object v;
        if (a == null || !a.TryGetValue(k, out v) || v == null) return def;
        try { return Convert.ToDouble(v); } catch { return def; }
    }

    static bool Bool(Dictionary<string, object> a, string k, bool def)
    {
        object v;
        if (a == null || !a.TryGetValue(k, out v) || v == null) return def;
        if (v is bool) return (bool)v;
        string s = Convert.ToString(v);
        if (s == "1" || s == "true" || s == "True" || s == "yes") return true;
        if (s == "0" || s == "false" || s == "False" || s == "no") return false;
        return def;
    }

    static int[] Ints(Dictionary<string, object> a, string k)
    {
        object v;
        if (a == null || !a.TryGetValue(k, out v) || v == null) return null;
        var arr = v as System.Collections.ArrayList;
        if (arr != null)
        {
            var r = new int[arr.Count];
            for (int i = 0; i < arr.Count; i++) r[i] = Convert.ToInt32(arr[i]);
            return r;
        }
        var oa = v as object[];
        if (oa != null)
        {
            var r = new int[oa.Length];
            for (int i = 0; i < oa.Length; i++) r[i] = Convert.ToInt32(oa[i]);
            return r;
        }
        var ia = v as int[];
        if (ia != null) return ia;
        var s = Convert.ToString(v);
        if (string.IsNullOrEmpty(s)) return null;
        var parts = s.Split(new[] { ',', ' ', 'x' }, StringSplitOptions.RemoveEmptyEntries);
        var list = new List<int>();
        foreach (var p in parts)
        {
            int n;
            if (int.TryParse(p, out n)) list.Add(n);
        }
        return list.Count > 0 ? list.ToArray() : null;
    }

    static byte[] ToJpeg(Bitmap bmp, long quality)
    {
        var enc = GetJpeg();
        var ep = new EncoderParameters(1);
        ep.Param[0] = new EncoderParameter(System.Drawing.Imaging.Encoder.Quality, quality);
        using (var ms = new MemoryStream())
        {
            bmp.Save(ms, enc, ep);
            return ms.ToArray();
        }
    }

    static ImageCodecInfo jpegCodec;
    static ImageCodecInfo GetJpeg()
    {
        if (jpegCodec != null) return jpegCodec;
        foreach (var c in ImageCodecInfo.GetImageEncoders())
            if (c.FormatID == ImageFormat.Jpeg.Guid) return jpegCodec = c;
        throw new Exception("No JPEG encoder.");
    }

    static string Sha(byte[] data)
    {
        using (var sha = SHA256.Create())
        {
            var h = sha.ComputeHash(data);
            var sb = new StringBuilder(h.Length * 2);
            for (int i = 0; i < h.Length; i++) sb.Append(h[i].ToString("x2"));
            return sb.ToString();
        }
    }
}

sealed class Server
{
    public Engine Engine;
    HttpListener http;
    Thread thread;
    public volatile bool Running;
    JavaScriptSerializer json = new JavaScriptSerializer { MaxJsonLength = 32 * 1024 * 1024 };

    public Server(Engine engine) { Engine = engine; }
    readonly Dictionary<string, string> mcpClients = new Dictionary<string, string>();
    volatile string lastMcpClient;

    public void Start()
    {
        int port = Config.Current.Port;
        http = new HttpListener();
        http.Prefixes.Add("http://127.0.0.1:" + port + "/");
        try { http.Start(); }
        catch (HttpListenerException)
        {
            port = Config.Current.Port + 1;
            Config.Current.Port = port;
            Config.Save();
            http = new HttpListener();
            http.Prefixes.Add("http://127.0.0.1:" + port + "/");
            http.Start();
        }
        Running = true;
        thread = new Thread(Loop);
        thread.IsBackground = true;
        thread.Start();
        Log.Write("listening on http://127.0.0.1:" + Config.Current.Port + "/ (token required)");
    }

    public void Stop()
    {
        Running = false;
        try { http.Close(); } catch { }
    }

    void Loop()
    {
        while (Running)
        {
            HttpListenerContext ctx;
            try { ctx = http.GetContext(); }
            catch { if (!Running) return; else continue; }
            ThreadPool.QueueUserWorkItem(_ => Serve(ctx));
        }
    }

    void Serve(HttpListenerContext ctx)
    {
        try
        {
            var req = ctx.Request;
            var res = ctx.Response;
            res.Headers["Server"] = "Reach/" + Paths.Version;
            string path = req.Url.AbsolutePath.TrimEnd('/').ToLowerInvariant();
            if (path == "") path = "/";

            if (req.HttpMethod == "GET" && (path == "/health" || path == "/status"))
            {
                var st = Engine.Status();
                WriteJson(res, 200, "{\"ok\":true,\"paused\":" + (Config.Current.Paused ? "true" : "false")
                    + ",\"port\":" + Config.Current.Port
                    + ",\"text\":" + json.Serialize(st.Text) + "}");
                return;
            }

            if (!Authed(req))
            {
                res.StatusCode = 401;
                res.AddHeader("WWW-Authenticate", "Bearer");
                WriteJson(res, 401, "{\"error\":\"unauthorized\"}");
                return;
            }

            if (req.HttpMethod == "POST" && (path == "/run" || path == "/cli"))
            {
                string body = ReadBody(req);
                var d = json.Deserialize<Dictionary<string, object>>(body) ?? new Dictionary<string, object>();
                string tool = GetStr(d, "tool", GetStr(d, "cmd", ""));
                var args = GetDict(d, "args") ?? GetDict(d, "arguments") ?? new Dictionary<string, object>();
                // flatten leftover keys
                foreach (var kv in d)
                    if (kv.Key != "tool" && kv.Key != "cmd" && kv.Key != "args" && kv.Key != "arguments")
                        args[kv.Key] = kv.Value;
                if (!args.ContainsKey("agent") && !string.IsNullOrEmpty(req.Headers["X-Reach-Agent"]))
                    args["agent"] = req.Headers["X-Reach-Agent"];
                var r = Engine.Run(tool, args);
                WriteAct(res, r, false);
                return;
            }

            if (path == "/mcp")
            {
                if (req.HttpMethod != "POST")
                {
                    res.StatusCode = 405;
                    res.Close();
                    return;
                }
                HandleMcp(req, res);
                return;
            }

            WriteJson(res, 404, "{\"error\":\"not found\"}");
        }
        catch (Exception ex)
        {
            Log.Write("http: " + ex.Message);
            try { ctx.Response.StatusCode = 500; ctx.Response.Close(); } catch { }
        }
    }

    void HandleMcp(HttpListenerRequest req, HttpListenerResponse res)
    {
        string body = ReadBody(req);
        Dictionary<string, object> msg;
        try { msg = json.Deserialize<Dictionary<string, object>>(body); }
        catch
        {
            WriteJson(res, 400, "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32700,\"message\":\"parse error\"},\"id\":null}");
            return;
        }
        if (msg == null) msg = new Dictionary<string, object>();
        object id = msg.ContainsKey("id") ? msg["id"] : null;
        string method = GetStr(msg, "method", "");
        var @params = GetDict(msg, "params") ?? new Dictionary<string, object>();

        if (method == "initialize")
        {
            var ci = GetDict(@params, "clientInfo");
            string client = GetStr(ci, "title", null) ?? GetStr(ci, "name", null);
            if (!string.IsNullOrWhiteSpace(client))
            {
                string sid = Guid.NewGuid().ToString("N");
                lock (mcpClients) { mcpClients[sid] = client; if (mcpClients.Count > 200) mcpClients.Clear(); }
                lastMcpClient = client;
                res.AddHeader("Mcp-Session-Id", sid);
            }
            var result = "{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{\"tools\":{\"listChanged\":false}},"
                + "\"serverInfo\":{\"name\":\"reach\",\"version\":\"" + Paths.Version + "\"},"
                + "\"instructions\":" + json.Serialize(McpInstructions()) + "}";
            WriteJson(res, 200, RpcOk(id, result));
            return;
        }
        if (method == "notifications/initialized" || method.StartsWith("notifications/"))
        {
            res.StatusCode = 202;
            res.Close();
            return;
        }
        if (method == "ping")
        {
            WriteJson(res, 200, RpcOk(id, "{}"));
            return;
        }
        if (method == "tools/list")
        {
            WriteJson(res, 200, RpcOk(id, "{\"tools\":[" + ToolSchemas() + "]}"));
            return;
        }
        if (method == "tools/call")
        {
            string name = GetStr(@params, "name", "");
            var args = GetDict(@params, "arguments") ?? new Dictionary<string, object>();
            if (!args.ContainsKey("agent"))
            {
                string who = req.Headers["X-Reach-Agent"];
                string sid = req.Headers["Mcp-Session-Id"];
                if (string.IsNullOrEmpty(who) && !string.IsNullOrEmpty(sid))
                    lock (mcpClients) mcpClients.TryGetValue(sid, out who);
                if (string.IsNullOrEmpty(who)) who = lastMcpClient;
                if (!string.IsNullOrEmpty(who)) args["agent"] = who;
            }
            var r = Engine.Run(name, args);
            WriteAct(res, r, true, id);
            return;
        }
        WriteJson(res, 200, "{\"jsonrpc\":\"2.0\",\"id\":" + JsonId(id)
            + ",\"error\":{\"code\":-32601,\"message\":\"unknown method\"}}");
    }

    string RpcOk(object id, string resultJson)
    {
        return "{\"jsonrpc\":\"2.0\",\"id\":" + JsonId(id) + ",\"result\":" + resultJson + "}";
    }

    string JsonId(object id)
    {
        if (id == null) return "null";
        if (id is string) return json.Serialize(id);
        return Convert.ToString(id);
    }

    void WriteAct(HttpListenerResponse res, ActResult r, bool mcp, object id = null)
    {
        if (!mcp)
        {
            var d = new Dictionary<string, object>();
            d["ok"] = r.Ok;
            d["text"] = r.Text;
            if (!string.IsNullOrEmpty(r.ImagePath)) d["path"] = r.ImagePath;
            if (r.Image != null) d["bytes"] = r.Image.Length;
            WriteJson(res, r.Ok ? 200 : 400, json.Serialize(d));
            return;
        }
        var content = new List<Dictionary<string, object>>();
        content.Add(new Dictionary<string, object> { { "type", "text" }, { "text", r.Text ?? "" } });
        if (r.Image != null && r.Image.Length > 0)
        {
            content.Add(new Dictionary<string, object> {
                { "type", "image" },
                { "data", Convert.ToBase64String(r.Image) },
                { "mimeType", r.Mime ?? "image/jpeg" }
            });
        }
        var result = new Dictionary<string, object>();
        result["content"] = content;
        if (!r.Ok) result["isError"] = true;
        WriteJson(res, 200, RpcOk(id, json.Serialize(result)));
    }

    bool Authed(HttpListenerRequest req)
    {
        string h = req.Headers["Authorization"];
        if (string.IsNullOrEmpty(h)) h = req.Headers["authorization"];
        if (string.IsNullOrEmpty(h)) return false;
        if (h.StartsWith("Bearer ", StringComparison.OrdinalIgnoreCase))
            h = h.Substring(7).Trim();
        return SlowEquals(h, Config.Current.Token);
    }

    static bool SlowEquals(string a, string b)
    {
        if (a == null || b == null || a.Length != b.Length) return false;
        int d = 0;
        for (int i = 0; i < a.Length; i++) d |= a[i] ^ b[i];
        return d == 0;
    }

    static string ReadBody(HttpListenerRequest req)
    {
        using (var sr = new StreamReader(req.InputStream, req.ContentEncoding ?? Encoding.UTF8))
            return sr.ReadToEnd();
    }

    static void WriteJson(HttpListenerResponse res, int code, string body)
    {
        byte[] buf = Encoding.UTF8.GetBytes(body ?? "");
        res.StatusCode = code;
        res.ContentType = "application/json; charset=utf-8";
        res.ContentLength64 = buf.Length;
        res.OutputStream.Write(buf, 0, buf.Length);
        res.OutputStream.Close();
    }

    static string GetStr(Dictionary<string, object> d, string k, string def)
    {
        object v;
        if (d == null || !d.TryGetValue(k, out v) || v == null) return def;
        return Convert.ToString(v) ?? def;
    }

    static Dictionary<string, object> GetDict(Dictionary<string, object> d, string k)
    {
        object v;
        if (d == null || !d.TryGetValue(k, out v) || v == null) return null;
        return v as Dictionary<string, object>;
    }

    public static string McpInstructions()
    {
        return "Drives this Windows desktop. Coordinates are screenshot image pixels; never scale them. "
            + "Prefer apps open <name> and ui find/invoke (refs eN stay valid) over pixel clicks. "
            + "Screenshot only when you need to see a result; 'unchanged' means nothing changed.";
    }

    public static string ToolSchemas()
    {
        return string.Join(",", new[] {
            Tool("screenshot", "JPEG of the desktop, a window, or a region, plus image size and cursor.",
                Props(
                    Prop("window", "string", "hwnd from apps"),
                    Prop("region", "array", "[x,y,w,h] zoom"),
                    Prop("max", "integer", "long edge px, default 1280"),
                    Prop("quality", "integer", "JPEG 20-95"),
                    Prop("force", "boolean", "image even if unchanged")
                ), null),
            Tool("apps", "List, open, focus, close, min/max/restore windows; list displays.",
                Props(
                    PropEnum("action", "list, open, focus, close, minimize, maximize, restore, displays", true),
                    Prop("name", "string", "app name, or title/process"),
                    Prop("window", "string", "hwnd"),
                    Prop("filter", "string", "list filter"),
                    Prop("new", "boolean", "open a new instance"),
                    Prop("json", "boolean", "full detail")
                ), "action"),
            Tool("mouse", "Click, move, drag, scroll at [x,y] image coords.",
                Props(
                    PropEnum("action", "click, right_click, double_click, middle_click, move, drag, scroll, position, down, up, wait", true),
                    Prop("coordinate", "array", "[x,y]"),
                    Prop("to", "array", "drag end [x,y]"),
                    PropEnum("direction", "up, down, left, right", false),
                    Prop("amount", "integer", "scroll notches, default 3"),
                    Prop("duration", "number", "wait seconds, max 10"),
                    Prop("fast", "boolean", "quick motion this call")
                ), "action"),
            Tool("keyboard", "Type text, or press chords like Return, ctrl+shift+t, win+r; sequences \"ctrl+c, ctrl+v\".",
                Props(
                    PropEnum("action", "type, key, hold", true),
                    Prop("text", "string", "text or chord"),
                    Prop("hold", "integer", "key: hold ms"),
                    Prop("duration", "number", "hold seconds, max 10"),
                    Prop("fast", "boolean", "quick typing this call")
                ), "action"),
            Tool("ui", "Accessibility tree. find/inspect return refs (eN) for invoke, set, read, toggle, focus. ref may also be a control name.",
                Props(
                    PropEnum("action", "inspect, find, invoke, set, read, toggle, focus", true),
                    Prop("query", "string", "find text"),
                    Prop("ref", "string", "eN or control name"),
                    Prop("value", "string", "text for set"),
                    Prop("window", "string", "hwnd, default focused window"),
                    Prop("limit", "integer", "default 60 inspect, 25 find"),
                    Prop("all", "boolean", "no filtering"),
                    Prop("json", "boolean", "full detail")
                ), "action")
        });
    }

    static string Tool(string name, string desc, string props, string required)
    {
        var sb = new StringBuilder();
        sb.Append("{\"name\":\"").Append(name).Append("\",\"description\":").Append(Q(desc))
          .Append(",\"inputSchema\":{\"type\":\"object\",\"properties\":{").Append(props).Append("}");
        if (!string.IsNullOrEmpty(required))
            sb.Append(",\"required\":[\"").Append(required).Append("\"]");
        sb.Append("}}");
        return sb.ToString();
    }

    static string Props(params string[] p) { return string.Join(",", p); }

    static string Prop(string name, string type, string desc)
    {
        if (type == "array")
            return "\"" + name + "\":{\"type\":\"array\",\"items\":{\"type\":\"integer\"},\"description\":" + Q(desc) + "}";
        return "\"" + name + "\":{\"type\":\"" + type + "\",\"description\":" + Q(desc) + "}";
    }

    static string PropEnum(string name, string values, bool requiredish)
    {
        var ev = values.Split(new[] { ',', ' ' }, StringSplitOptions.RemoveEmptyEntries);
        var sb = new StringBuilder();
        sb.Append("\"").Append(name).Append("\":{\"type\":\"string\",\"enum\":[");
        for (int i = 0; i < ev.Length; i++)
        {
            if (i > 0) sb.Append(",");
            sb.Append(Q(ev[i]));
        }
        sb.Append("]}");
        return sb.ToString();
    }

    static string Q(string s)
    {
        return "\"" + (s ?? "").Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\n", "\\n") + "\"";
    }
}

sealed class Tray : ApplicationContext
{
    NotifyIcon icon;
    public Server Server;
    Engine engine;

    public Tray(Engine engine, Server server)
    {
        this.engine = engine;
        this.Server = server;
        icon = new NotifyIcon();
        icon.Icon = MakeIcon(false);
        icon.Text = "Reach: desktop control";
        icon.Visible = true;
        icon.ContextMenuStrip = BuildMenu();
        icon.DoubleClick += delegate { ShowStatusBalloon(); };
        var trayCtx = SynchronizationContext.Current;
        UserYield.StateChanged += delegate (bool p)
        {
            if (trayCtx == null) return;
            trayCtx.Post(delegate
            {
                try
                {
                    icon.Icon = MakeIcon(p || Config.Current.Paused);
                    icon.Text = p ? "Reach: paused, you're using the PC"
                        : (Config.Current.Paused ? "Reach: paused" : "Reach: desktop control");
                }
                catch { }
            }, null);
        };
        ShowStatusBalloon();
    }

    ContextMenuStrip BuildMenu()
    {
        var m = new ContextMenuStrip();
        var status = new ToolStripMenuItem(Config.Current.Paused ? "Reach is paused" : "Reach is running");
        status.Enabled = false;
        m.Items.Add(status);
        m.Items.Add(new ToolStripSeparator());
        var pause = new ToolStripMenuItem(Config.Current.Paused ? "Resume" : "Pause");
        pause.Click += delegate
        {
            Config.Current.Paused = !Config.Current.Paused;
            Config.Save();
            icon.Icon = MakeIcon(Config.Current.Paused);
            icon.Text = Config.Current.Paused ? "Reach: paused" : "Reach: desktop control";
            icon.ContextMenuStrip = BuildMenu();
            icon.ShowBalloonTip(1500, "Reach", Config.Current.Paused ? "Paused. Nothing can control this PC until you resume." : "Running.", ToolTipIcon.Info);
        };
        m.Items.Add(pause);
        var usage = new ToolStripMenuItem("Open usage log");
        usage.Click += delegate
        {
            Directory.CreateDirectory(Paths.Dir);
            if (!File.Exists(Paths.Usage)) File.WriteAllText(Paths.Usage, "");
            Process.Start("explorer.exe", Paths.Dir);
        };
        m.Items.Add(usage);
        m.Items.Add(new ToolStripSeparator());
        var exit = new ToolStripMenuItem("Exit");
        exit.Click += delegate { Exit(); };
        m.Items.Add(exit);
        m.Opening += delegate
        {
            status.Text = Config.Current.Paused ? "Reach is paused" : "Reach is running";
            pause.Text = Config.Current.Paused ? "Resume" : "Pause";
        };
        return m;
    }

    void ShowStatusBalloon()
    {
        var st = engine.Status();
        icon.ShowBalloonTip(1800, "Reach", st.Text, ToolTipIcon.Info);
    }

    public void Exit()
    {
        try { Server.Stop(); } catch { }
        icon.Visible = false;
        icon.Dispose();
        Application.Exit();
    }

    static Icon MakeIcon(bool paused)
    {
        var bmp = ReachLogo.Render(32, paused);
        IntPtr h = bmp.GetHicon();
        var ic = Icon.FromHandle(h);
        var clone = (Icon)ic.Clone();
        NativeDestroyIcon(h);
        bmp.Dispose();
        return clone;
    }

    [DllImport("user32.dll", EntryPoint = "DestroyIcon")]
    static extern bool NativeDestroyIcon(IntPtr h);
}

static class Cli
{
    static readonly JavaScriptSerializer Json = new JavaScriptSerializer { MaxJsonLength = 16 * 1024 * 1024 };

    public static int Run(string[] args)
    {
        if (args.Length == 0 || IsHelp(args[0])) { PrintHelp(); return 0; }
        string cmd = args[0].ToLowerInvariant();
        if (cmd == "serve" || cmd == "start") return Program.Serve();
        if (cmd == "stop") return StopServer();
        if (cmd == "help" || cmd == "-h" || cmd == "--help") { PrintHelp(); return 0; }

        string agent = Environment.GetEnvironmentVariable("REACH_AGENT");
        bool noCursor = false, noYield = false, fast = false;
        args = StripGlobalFlags(args, ref agent, ref noCursor, ref noYield, ref fast);
        if (args.Length == 0) { PrintHelp(); return 0; }
        cmd = args[0].ToLowerInvariant();
        var payload = BuildPayload(args);
        if (!string.IsNullOrWhiteSpace(agent) || noCursor || noYield || fast)
        {
            if (fast) { var pf = Json.Deserialize<Dictionary<string, object>>(payload); pf["fast"] = true; payload = Json.Serialize(pf); }
            if (noYield) { var pd0 = Json.Deserialize<Dictionary<string, object>>(payload); pd0["no_yield"] = true; payload = Json.Serialize(pd0); }
            var pd = Json.Deserialize<Dictionary<string, object>>(payload);
            if (!string.IsNullOrWhiteSpace(agent)) pd["agent"] = agent.Trim();
            if (noCursor) pd["no_cursor"] = true;
            payload = Json.Serialize(pd);
        }
        string body;
        try { body = Post("/run", payload); }
        catch (WebException ex)
        {
            if (ex.Status != WebExceptionStatus.ConnectFailure) throw;
            EnsureServer();
            body = Post("/run", payload);
        }
        Dictionary<string, object> d;
        try { d = Json.Deserialize<Dictionary<string, object>>(body); }
        catch
        {
            Console.Error.WriteLine(body);
            return 1;
        }
        bool ok = true;
        if (d != null && d.ContainsKey("ok"))
            try { ok = Convert.ToBoolean(d["ok"]); } catch { }
        if (d != null && d.ContainsKey("text"))
            Console.WriteLine(Convert.ToString(d["text"]));
        else
            Console.WriteLine(body);
        if (d != null && d.ContainsKey("path"))
            Console.WriteLine("path " + d["path"]);
        return ok ? 0 : 2;
    }

    // --agent "<name>" and --no-cursor: anywhere before the command, and after it
    // too unless the command takes free text (type, key, open, ui, apps).
    static string[] StripGlobalFlags(string[] args, ref string agent, ref bool noCursor, ref bool noYield, ref bool fast)
    {
        var outList = new List<string>();
        string cmd = null;
        for (int i = 0; i < args.Length; i++)
        {
            string a = args[i];
            bool freeText = cmd == "type" || cmd == "key" || cmd == "open" || cmd == "ui" || cmd == "apps" || cmd == "windows";
            if (!freeText)
            {
                if ((a == "--agent" || a == "-a") && i + 1 < args.Length) { agent = args[++i]; continue; }
                if (a.StartsWith("--agent=")) { agent = a.Substring(8); continue; }
                if (a == "--no-cursor") { noCursor = true; continue; }
                if (a == "--no-yield") { noYield = true; continue; }
                if (a == "--fast") { fast = true; continue; }
            }
            if (cmd == null) cmd = a.ToLowerInvariant();
            outList.Add(a);
        }
        return outList.ToArray();
    }

    static bool IsHelp(string s)
    {
        s = s.ToLowerInvariant();
        return s == "-h" || s == "--help" || s == "help" || s == "/?";
    }

    static string BuildPayload(string[] args)
    {
        string cmd = args[0].ToLowerInvariant();
        var d = new Dictionary<string, object>();
        switch (cmd)
        {
            case "screenshot":
            case "shot":
                d["tool"] = "screenshot";
                ParseFlags(args, 1, d);
                return Json.Serialize(d);
            case "zoom":
                d["tool"] = "screenshot";
                if (args.Length >= 5)
                    d["region"] = new[] { int.Parse(args[1]), int.Parse(args[2]), int.Parse(args[3]), int.Parse(args[4]) };
                else throw new Exception("usage: reach zoom X Y W H");
                ParseFlags(args, 5, d);
                return Json.Serialize(d);
            case "apps":
            case "windows":
                d["tool"] = "apps";
                d["action"] = "list";
                args = TakeListFlags(args, d);
                if (args.Length > 1) d["filter"] = Join(args, 1);
                return Json.Serialize(d);
            case "open":
                d["tool"] = "apps";
                d["action"] = "open";
                d["name"] = Join(args, 1);
                if (string.IsNullOrEmpty((string)d["name"])) throw new Exception("usage: reach open <app>");
                return Json.Serialize(d);
            case "focus":
                d["tool"] = "apps"; d["action"] = "focus"; d["window"] = args.Length > 1 ? args[1] : "";
                return Json.Serialize(d);
            case "close":
                d["tool"] = "apps"; d["action"] = "close"; d["window"] = args.Length > 1 ? args[1] : "";
                return Json.Serialize(d);
            case "minimize":
                d["tool"] = "apps"; d["action"] = "minimize"; d["window"] = args.Length > 1 ? args[1] : "";
                return Json.Serialize(d);
            case "maximize":
                d["tool"] = "apps"; d["action"] = "maximize"; d["window"] = args.Length > 1 ? args[1] : "";
                return Json.Serialize(d);
            case "displays":
                d["tool"] = "apps"; d["action"] = "displays";
                return Json.Serialize(d);
            case "click":
                d["tool"] = "mouse"; d["action"] = "click"; NeedXY(args, d);
                return Json.Serialize(d);
            case "right-click":
            case "rightclick":
                d["tool"] = "mouse"; d["action"] = "right_click"; NeedXY(args, d);
                return Json.Serialize(d);
            case "double-click":
            case "doubleclick":
                d["tool"] = "mouse"; d["action"] = "double_click"; NeedXY(args, d);
                return Json.Serialize(d);
            case "move":
                d["tool"] = "mouse"; d["action"] = "move"; NeedXY(args, d);
                return Json.Serialize(d);
            case "drag":
                d["tool"] = "mouse"; d["action"] = "drag";
                if (args.Length < 5) throw new Exception("usage: reach drag X1 Y1 X2 Y2");
                d["coordinate"] = new[] { int.Parse(args[1]), int.Parse(args[2]) };
                d["to"] = new[] { int.Parse(args[3]), int.Parse(args[4]) };
                return Json.Serialize(d);
            case "scroll":
                d["tool"] = "mouse"; d["action"] = "scroll";
                d["direction"] = args.Length > 1 ? args[1] : "down";
                if (args.Length > 2) d["amount"] = int.Parse(args[2]);
                if (args.Length > 4)
                    d["coordinate"] = new[] { int.Parse(args[3]), int.Parse(args[4]) };
                return Json.Serialize(d);
            case "type":
                d["tool"] = "keyboard"; d["action"] = "type"; d["text"] = Join(args, 1);
                return Json.Serialize(d);
            case "key":
                {
                    d["tool"] = "keyboard"; d["action"] = "key";
                    var rest = new List<string>();
                    for (int i = 1; i < args.Length; i++)
                    {
                        if (args[i] == "--hold" && i + 1 < args.Length) { d["hold"] = int.Parse(args[++i]); continue; }
                        if (args[i] == "--fast") { d["fast"] = true; continue; }
                        rest.Add(args[i]);
                    }
                    d["text"] = string.Join(" ", rest.ToArray());
                    return Json.Serialize(d);
                }
            case "motion":
                d["tool"] = "motion"; d["value"] = args.Length > 1 ? args[1].ToLowerInvariant() : "status";
                return Json.Serialize(d);
            case "ui":
                d["tool"] = "ui";
                if (args.Length > 1 && (args[1] == "find" || args[1] == "inspect")) args = TakeListFlags(args, d);
                d["action"] = args.Length > 1 ? args[1] : "inspect";
                if (d["action"].ToString() == "find" && args.Length > 2) d["query"] = Join(args, 2);
                else if (d["action"].ToString() == "set" && args.Length > 3) { d["ref"] = args[2]; d["value"] = Join(args, 3); }
                else if (args.Length > 2) d["ref"] = args[2];
                return Json.Serialize(d);
            case "status":
                d["tool"] = "status";
                TakeListFlags(args, d);
                return Json.Serialize(d);
            case "pause-on-user":
            case "pauseonuser":
                d["tool"] = "pause_on_user";
                d["action"] = args.Length > 1 ? args[1].ToLowerInvariant() : "status";
                if (args.Length > 2) d["value"] = int.Parse(args[2]);
                return Json.Serialize(d);
            case "cursor":
                d["tool"] = "cursor";
                d["action"] = args.Length > 1 ? args[1].ToLowerInvariant() : "status";
                if (args.Length > 2) d["value"] = args[2];
                return Json.Serialize(d);
            case "usage":
                d["tool"] = "usage";
                if (args.Length > 1) d["limit"] = int.Parse(args[1]);
                return Json.Serialize(d);
            case "pause":
                d["tool"] = "pause";
                return Json.Serialize(d);
            case "resume":
                d["tool"] = "resume";
                return Json.Serialize(d);
            default:
                throw new Exception("Unknown command `" + cmd + "`. Run `reach help`.");
        }
    }

    // --json, --all, --limit N, --window X for list-style commands.
    static string[] TakeListFlags(string[] args, Dictionary<string, object> d)
    {
        var rest = new List<string>();
        for (int i = 0; i < args.Length; i++)
        {
            string a = args[i];
            if (a == "--json") d["json"] = true;
            else if (a == "--all") d["all"] = true;
            else if (a == "--limit" && i + 1 < args.Length) d["limit"] = int.Parse(args[++i]);
            else if ((a == "--window" || a == "-w") && i + 1 < args.Length) d["window"] = args[++i];
            else rest.Add(a);
        }
        return rest.ToArray();
    }

    static void NeedXY(string[] args, Dictionary<string, object> d)
    {
        if (args.Length < 3) throw new Exception("usage: reach " + args[0] + " X Y");
        d["coordinate"] = new[] { int.Parse(args[1]), int.Parse(args[2]) };
    }

    static void ParseFlags(string[] args, int start, Dictionary<string, object> d)
    {
        for (int i = start; i < args.Length; i++)
        {
            string a = args[i];
            if (a == "--force" || a == "-f") d["force"] = true;
            else if (a == "--include-overlay") d["include_overlay"] = true;
            else if (a == "--max" && i + 1 < args.Length) d["max"] = int.Parse(args[++i]);
            else if ((a == "--quality" || a == "-q") && i + 1 < args.Length) d["quality"] = int.Parse(args[++i]);
            else if ((a == "--window" || a == "-w") && i + 1 < args.Length) d["window"] = args[++i];
            else if ((a == "--region" || a == "-r") && i + 1 < args.Length)
            {
                var p = args[++i].Split(new[] { ',', 'x' }, StringSplitOptions.RemoveEmptyEntries);
                d["region"] = new[] { int.Parse(p[0]), int.Parse(p[1]), int.Parse(p[2]), int.Parse(p[3]) };
            }
        }
    }

    static string Join(string[] args, int start)
    {
        if (args.Length <= start) return "";
        return string.Join(" ", args, start, args.Length - start);
    }

    public static string Post(string path, string body)
    {
        var req = (HttpWebRequest)WebRequest.Create("http://127.0.0.1:" + Config.Current.Port + path);
        req.Method = "POST";
        req.ContentType = "application/json";
        req.Headers["Authorization"] = "Bearer " + Config.Current.Token;
        // Long enough for a pause-on-user wait plus the command itself.
        int t = Math.Max(30000, Config.Current.UserPauseTimeoutMs + 30000);
        req.Timeout = t;
        req.ReadWriteTimeout = t;
        req.Proxy = null;
        req.KeepAlive = false;
        var bytes = Encoding.UTF8.GetBytes(body ?? "{}");
        req.ContentLength = bytes.Length;
        using (var s = req.GetRequestStream()) s.Write(bytes, 0, bytes.Length);
        try
        {
            using (var resp = (HttpWebResponse)req.GetResponse())
            using (var sr = new StreamReader(resp.GetResponseStream()))
                return sr.ReadToEnd();
        }
        catch (WebException ex)
        {
            if (ex.Response != null)
            {
                using (var sr = new StreamReader(ex.Response.GetResponseStream()))
                    return sr.ReadToEnd();
            }
            throw;
        }
    }

    public static bool Health(out string text)
    {
        text = "";
        try
        {
            var req = (HttpWebRequest)WebRequest.Create("http://127.0.0.1:" + Config.Current.Port + "/health");
            req.Method = "GET";
            req.Timeout = 800;
            req.ReadWriteTimeout = 800;
            req.Proxy = null;
            req.KeepAlive = false;
            using (var resp = (HttpWebResponse)req.GetResponse())
            using (var sr = new StreamReader(resp.GetResponseStream()))
            {
                text = sr.ReadToEnd();
                return resp.StatusCode == HttpStatusCode.OK;
            }
        }
        catch { return false; }
    }

    public static void EnsureServer()
    {
        string t;
        if (Health(out t)) return;
        string exe = System.Reflection.Assembly.GetExecutingAssembly().Location;
        if (string.IsNullOrEmpty(exe)) exe = Application.ExecutablePath;
        var psi = new ProcessStartInfo(exe, "serve");
        psi.UseShellExecute = true;
        psi.CreateNoWindow = true;
        psi.WindowStyle = ProcessWindowStyle.Hidden;
        Process.Start(psi);
        for (int i = 0; i < 25; i++)
        {
            Thread.Sleep(150);
            if (Health(out t)) return;
        }
        throw new Exception("Reach server did not start. Try `reach serve`.");
    }

    static int StopServer()
    {
        Console.WriteLine("Use the tray icon > Exit to stop Reach.");
        return 0;
    }

    public static void PrintHelp()
    {
        Console.WriteLine(@"reach " + Paths.Version + @" - desktop control for AI agents

Apps and windows
  reach open notepad             launch, or focus if already open
  reach apps [filter] [--json]   one line per window: hwnd title [process] x,y wxh
  reach focus 0xHWND | close 0xHWND | displays

Screen
  reach screenshot [--max 1280] [--quality 72] [--region x,y,w,h] [--force]
  reach zoom X Y W H

Mouse and keyboard (screenshot image coordinates)
  reach click X Y | double-click X Y | right-click X Y | move X Y
  reach drag X1 Y1 X2 Y2 | scroll down 3 [X Y]
  reach type hello world | key ctrl+shift+t | key ""ctrl+c, ctrl+v"" | key win+r --hold 500
  reach motion smooth | fast | instant        how input moves (default smooth)

UI controls (preferred over pixels)
  reach ui find Save [--limit 25] [--json] [--all]
  reach ui inspect [--limit 60] [--json] [--all] [--window 0xHWND]
  reach ui invoke e3 | set e3 text | read e3 | toggle e3
  reach ui invoke Save           a name works too (resolved fresh)

Agent pill (name tag next to the real cursor while an agent acts)
  reach --agent ""Claude"" click X Y      or set REACH_AGENT
  reach cursor on | off | status | show [move|type|click] | ripple on|off
  --no-cursor                             skip it for one command
  reach screenshot --include-overlay      debug: capture with it visible

Pause on user
  reach pause-on-user on | off | status | idle <ms> | timeout <ms>
  --no-yield                              act now even if you are using the PC
  --fast                                  quick glides and bulk typing for this call

Server
  reach status [--json] | pause | resume | usage | serve | help");
    }
}


// ---------------------------------------------------------------------------
// Agent cursor: a click-through, capture-excluded overlay that shows where an
// agent is pointing, with its name in a pill (Figma / computer-use style).
// One small layered window per agent name, all on a dedicated UI thread so the
// engine's blocking work never stalls the animation.
// ---------------------------------------------------------------------------
static class AgentCursor
{
    [ThreadStatic] public static string Current;   // set per call by Engine.Run
    static Thread thread;
    static OverlayHost host;
    static SynchronizationContext hostCtx;
    static readonly object startLock = new object();
    public static volatile bool IncludeInCapture;

    public static string Resolve(Dictionary<string, object> args, string fallback)
    {
        string n = null;
        object v;
        if (args != null && args.TryGetValue("agent", out v) && v != null) n = Convert.ToString(v);
        if (string.IsNullOrWhiteSpace(n)) n = fallback;
        if (string.IsNullOrWhiteSpace(n)) n = "Agent";
        n = n.Trim();
        if (n.Length > 28) n = n.Substring(0, 27) + "…";
        return n;
    }

    public static bool Enabled { get { return Config.Current.AgentCursor; } }

    static OverlayHost Host()
    {
        if (host != null) return host;
        lock (startLock)
        {
            if (host != null) return host;
            var ready = new ManualResetEvent(false);
            thread = new Thread(delegate ()
            {
                try { SetThreadDpiAwarenessContext(new IntPtr(-4)); } catch { } // per-monitor v2, crisp on every screen
                var ctx = new WindowsFormsSynchronizationContext();
                SynchronizationContext.SetSynchronizationContext(ctx);
                hostCtx = ctx;
                host = new OverlayHost();
                ready.Set();
                Application.Run();
            });
            thread.IsBackground = true;
            thread.SetApartmentState(ApartmentState.STA);
            thread.Name = "reach-agent-cursor";
            thread.Start();
            ready.WaitOne(3000);
            return host;
        }
    }

    // kind: "move", "click", "type"
    public static void At(Point screen, string kind)
    {
        if (!Enabled || NoCursorThisCall) return;
        string agent = Current ?? "Agent";
        try
        {
            var h = Host();
            if (h == null) return;
            hostCtx.Post(delegate { h.Show(agent, screen, kind); }, null);
        }
        catch (Exception ex) { Log.Write("agent cursor: " + ex.Message); }
    }

    [ThreadStatic] public static bool NoCursorThisCall;

    // reach pill-preview --out DIR: renders the pill demo timeline to PNG frames (60 fps)
    // plus stills, entirely offscreen. Also draws a fake desktop, a glide, typing and chords.
    public static string Preview(string dir, string agent, float scale)
    {
        Directory.CreateDirectory(dir);
        int W = (int)(560 * scale), H = (int)(230 * scale);
        var pill = PillWindow.ForPreview(agent, scale, 900);
        string typed = "hello from Reach";
        // timeline (ms)
        var A = new PointF(70 * scale, 60 * scale); var B = new PointF(250 * scale, 112 * scale);
        int frames = 0;
        var stills = new Dictionary<int, string> { { 760, "pill-still-move.png" }, { 1900, "pill-still-typing.png" }, { 3350, "pill-still-chord.png" }, { 5600, "pill-still-paused.png" } };
        using (var bg = new Bitmap(W, H, PixelFormat.Format32bppArgb))
        {
            for (long t = 0; t <= 7600; t += 1000 / 60)
            {
                // events
                if (t == 0) pill.SetClock(0);
                pill.SetClock(t);
                if (Hit(t, 0)) pill.Act("move");
                if (Hit(t, 900)) pill.Act("click");
                if (Hit(t, 1300)) pill.Act("type");
                if (t > 1300 && t < 2700 && (t / 300) != ((t - 16) / 300)) pill.Act("type");
                if (Hit(t, 3000)) pill.Act("key:Ctrl + Shift + T");
                if (Hit(t, 3900)) pill.Act("key:Ctrl + C");
                if (Hit(t, 4500)) pill.Act("key:Ctrl + V");
                if (Hit(t, 5200)) pill.SetPaused(true);
                if (Hit(t, 6300)) pill.SetPaused(false);
                // cursor glide A -> B (eased, curved), like MoveTo
                double gt = Math.Max(0, Math.Min(1, (t - 150) / 420.0));
                double e = gt < 0.5 ? 4 * gt * gt * gt : 1 - Math.Pow(-2 * gt + 2, 3) / 2, u = 1 - e;
                float kx = (A.X + B.X) / 2 + 20 * scale, ky = (A.Y + B.Y) / 2 - 40 * scale;
                var c = new PointF((float)(u * u * A.X + 2 * u * e * kx + e * e * B.X), (float)(u * u * A.Y + 2 * u * e * ky + e * e * B.Y));
                int n = t < 1300 ? 0 : (int)Math.Min(typed.Length, (t - 1300) / 70);
                bool selected = t >= 3900 && t < 4500;
                int ci = (int)Math.Round(c.X), cj = (int)Math.Round(c.Y);
                bool vis = pill.Step(ci, cj, 0);
                using (var g = Graphics.FromImage(bg))
                {
                    DrawBackdrop(g, W, H, scale, typed.Substring(0, n) + (t >= 4500 && t < 9999 ? "" : ""), selected, t);
                    if (vis && pill.Frame != null && pill.PvAlpha > 0)
                    {
                        var cm = new ColorMatrix { Matrix33 = pill.PvAlpha / 255f };
                        using (var ia = new ImageAttributes())
                        {
                            ia.SetColorMatrix(cm);
                            var f = pill.Frame;
                            g.DrawImage(f, new Rectangle(pill.PvX, pill.PvY, f.Width, f.Height), 0, 0, f.Width, f.Height, GraphicsUnit.Pixel, ia);
                        }
                    }
                    DrawArrow(g, c.X, c.Y, scale);
                }
                bg.Save(Path.Combine(dir, "f" + frames.ToString("0000") + ".png"), ImageFormat.Png);
                foreach (var kv in stills)
                    if (Hit(t, kv.Key)) bg.Save(Path.Combine(dir, kv.Value), ImageFormat.Png);
                frames++;
            }
        }
        return frames + " frames at 60 fps, " + W + "x" + H + " -> " + dir;
    }

    static bool Hit(long t, long at) { return t <= at && at < t + 1000 / 60; }

    static void DrawBackdrop(Graphics g, int W, int H, float s, string text, bool selected, long t)
    {
        g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.HighQuality;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
        using (var br = new System.Drawing.Drawing2D.LinearGradientBrush(new Rectangle(0, 0, W, H), Color.FromArgb(205, 216, 236), Color.FromArgb(226, 214, 240), 20f))
            g.FillRectangle(br, 0, 0, W, H);
        var win = new RectangleF(150 * s, 70 * s, 360 * s, 120 * s);
        using (var sh = new SolidBrush(Color.FromArgb(28, 20, 20, 40))) g.FillRectangle(sh, win.X + 2 * s, win.Y + 5 * s, win.Width, win.Height);
        g.FillRectangle(Brushes.White, win);
        using (var tb = new SolidBrush(Color.FromArgb(243, 244, 248))) g.FillRectangle(tb, win.X, win.Y, win.Width, 26 * s);
        using (var f = new Font("Segoe UI", 11f * s, GraphicsUnit.Pixel))
        using (var b = new SolidBrush(Color.FromArgb(90, 96, 110)))
            g.DrawString("Untitled - Notes", f, b, win.X + 10 * s, win.Y + 6 * s);
        var box = new RectangleF(win.X + 14 * s, win.Y + 40 * s, win.Width - 28 * s, 30 * s);
        using (var p = new Pen(Color.FromArgb(150, 99, 120, 250), 1.5f * s)) g.DrawRectangle(p, box.X, box.Y, box.Width, box.Height);
        using (var f = new Font("Segoe UI", 14f * s, GraphicsUnit.Pixel))
        {
            var sz = g.MeasureString(text, f);
            if (selected && text.Length > 0)
                using (var sb = new SolidBrush(Color.FromArgb(200, 191, 219, 254))) g.FillRectangle(sb, box.X + 6 * s, box.Y + 5 * s, sz.Width - 4 * s, 20 * s);
            using (var b = new SolidBrush(Color.FromArgb(20, 24, 34))) g.DrawString(text, f, b, box.X + 6 * s, box.Y + 5 * s);
            if ((t / 500) % 2 == 0)
                using (var cp = new Pen(Color.FromArgb(20, 24, 34), 1f * s)) g.DrawLine(cp, box.X + 4 * s + sz.Width, box.Y + 7 * s, box.X + 4 * s + sz.Width, box.Y + 23 * s);
        }
    }

    static void DrawArrow(Graphics g, float x, float y, float s)
    {
        var pts = new[] { new PointF(0, 0), new PointF(0, 17), new PointF(4.2f, 13), new PointF(7.2f, 19.5f), new PointF(9.6f, 18.4f), new PointF(6.7f, 12.2f), new PointF(12, 12.2f) };
        for (int i = 0; i < pts.Length; i++) pts[i] = new PointF(x + pts[i].X * s, y + pts[i].Y * s);
        g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.HighQuality;
        g.FillPolygon(Brushes.White, pts);
        using (var p = new Pen(Color.Black, 1.1f * s) { LineJoin = System.Drawing.Drawing2D.LineJoin.Round }) g.DrawPolygon(p, pts);
    }
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr ctx);

    public static void Paused(string agent, bool paused)
    {
        if (!Enabled) return;
        agent = string.IsNullOrWhiteSpace(agent) ? "Agent" : agent;
        try
        {
            var h = Host();
            if (h == null) return;
            hostCtx.Post(delegate { h.SetPaused(agent, paused); }, null);
        }
        catch { }
    }

    public static void AtCursor(string kind)
    {
        Native.POINT p;
        if (Native.GetCursorPos(out p)) At(new Point(p.X, p.Y), kind);
    }

    // The pill rides with the real cursor, so typing shows there too.
    public static void AtCaret(string kind) { AtCursor(kind); }

    // Show the overlay in captures for a moment (debug: screenshot --include-overlay).
    public static void SetCaptureVisible(bool visible)
    {
        IncludeInCapture = visible;
        var h = host;
        if (h == null) return;
        try { hostCtx.Send(delegate { h.ApplyAffinity(); }, null); } catch { }
    }

    public static void HideAll()
    {
        var h = host;
        if (h == null) return;
        try { hostCtx.Post(delegate { h.HideAll(); }, null); } catch { }
    }



    // Invisible control that owns the overlay thread's message loop and the windows.
    sealed class OverlayHost
    {
        readonly Dictionary<string, PillWindow> windows = new Dictionary<string, PillWindow>(StringComparer.OrdinalIgnoreCase);
        readonly List<string> order = new List<string>();
        readonly System.Windows.Forms.Timer tick = new System.Windows.Forms.Timer();

        public OverlayHost()
        {
            tick.Interval = 16;   // ~60 fps; stops once every pill is hidden
            tick.Tick += delegate { Step(); };
        }

        PillWindow Get(string agent, bool create)
        {
            PillWindow w;
            if (windows.TryGetValue(agent, out w) && !w.IsDisposed) return w;
            if (!create) return null;
            w = new PillWindow(agent);
            windows[agent] = w;
            order.Remove(agent);
            order.Add(agent);
            return w;
        }

        public void Show(string agent, Point target, string kind)
        {
            Get(agent, true).Act(kind);
            if (!tick.Enabled) tick.Start();
        }

        public void SetPaused(string agent, bool paused)
        {
            var w = Get(agent, paused);
            if (w == null) return;
            w.SetPaused(paused);
            if (!tick.Enabled) tick.Start();
        }

        public void ApplyAffinity()
        {
            foreach (var w in windows.Values) if (!w.IsDisposed) w.ApplyAffinity();
        }

        public void HideAll()
        {
            foreach (var w in windows.Values) if (!w.IsDisposed) w.Hide();
        }

        void Step()
        {
            Native.POINT c;
            if (!Native.GetCursorPos(out c)) return;
            bool any = false;
            int slot = 0;
            foreach (var name in order)
            {
                PillWindow w;
                if (!windows.TryGetValue(name, out w) || w.IsDisposed) continue;
                if (w.Step(c.X, c.Y, slot)) { any = true; slot++; }
            }
            if (!any) tick.Stop();
        }
    }

    // A light name pill that rides next to the real Windows cursor (below-right of the
    // arrow, never over its tip). One soft style for every agent. Per-pixel alpha via
    // UpdateLayeredWindow, rendered at the cursor's monitor DPI.
    sealed class PillWindow : Form
    {
        const int WS_EX_LAYERED = 0x80000, WS_EX_TRANSPARENT = 0x20, WS_EX_TOOLWINDOW = 0x80,
                  WS_EX_NOACTIVATE = 0x8000000, WS_EX_TOPMOST = 0x8;
        const uint WDA_NONE = 0, WDA_EXCLUDEFROMCAPTURE = 0x11;
        const int RippleMs = 520, IdleMs = 3500, FadeMs = 320;

        static readonly Color Bg = Color.FromArgb(242, 255, 255, 255), Bg2 = Color.FromArgb(236, 244, 246, 252),
            BgPaused = Color.FromArgb(244, 255, 251, 240), BgPaused2 = Color.FromArgb(238, 255, 243, 222),
            Border = Color.FromArgb(38, 15, 23, 42), BorderPaused = Color.FromArgb(80, 214, 150, 40),
            Ink = Color.FromArgb(17, 20, 30), Ink2 = Color.FromArgb(92, 100, 118), Ink2Paused = Color.FromArgb(150, 92, 12),
            Work = Color.FromArgb(99, 120, 250), Typing = Color.FromArgb(99, 102, 241), Amber = Color.FromArgb(245, 158, 11),
            Ring = Color.FromArgb(99, 120, 250);
        static string nameFamily, subFamily;
        static FontStyle nameStyle = FontStyle.Regular;

        readonly string agent;
        float s = 1;                  // DPI scale of the cursor's monitor
        int margin;                   // cursor hotspot sits at (margin, margin) inside the frame
        PointF pos;
        bool placed, paused;
        long rippleStart = -100000, lastAct, appearStart;
        string kind = "move";
        Bitmap frame;
        IntPtr hbmp;
        bool sizedOnce;

        public PillWindow(string agent)
        {
            this.agent = agent;
            FormBorderStyle = FormBorderStyle.None;
            ShowInTaskbar = false;
            StartPosition = FormStartPosition.Manual;
            Size = new Size(1, 1);
            Location = new Point(-32000, -32000);
            if (nameFamily == null) PickFonts();
        }

        static void PickFonts()
        {
            var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            using (var fc = new System.Drawing.Text.InstalledFontCollection())
                foreach (var f in fc.Families) names.Add(f.Name);
            // GDI+ draws variable-font instances at their default (thin) weight, so use the static faces.
            if (names.Contains("Segoe UI Semibold")) nameFamily = "Segoe UI Semibold";
            else { nameFamily = "Segoe UI"; nameStyle = FontStyle.Bold; }
            subFamily = "Segoe UI";
        }

        protected override CreateParams CreateParams
        {
            get
            {
                var cp = base.CreateParams;
                cp.ExStyle |= WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
                return cp;
            }
        }
        protected override bool ShowWithoutActivation { get { return true; } }
        protected override void WndProc(ref Message m)
        {
            if (m.Msg == 0x84) { m.Result = (IntPtr)(-1); return; }  // WM_NCHITTEST -> HTTRANSPARENT
            if (m.Msg == 0x21) { m.Result = (IntPtr)3; return; }     // WM_MOUSEACTIVATE -> MA_NOACTIVATE
            base.WndProc(ref m);
        }

        public void ApplyAffinity()
        {
            if (IsHandleCreated) SetWindowDisplayAffinity(Handle, AgentCursor.IncludeInCapture ? WDA_NONE : WDA_EXCLUDEFROMCAPTURE);
        }

        // Offscreen preview: a virtual clock, no window handle, nothing shown.
        bool preview, pvVisible;
        long fakeNow;
        int idleMs = IdleMs;
        public int PvX, PvY;
        public byte PvAlpha;
        public Bitmap Frame { get { return frame; } }
        public bool PvVisible { get { return pvVisible; } }
        public static PillWindow ForPreview(string agent, float scale, int idle)
        {
            var w = new PillWindow(agent);
            w.preview = true; w.s = scale; w.margin = (int)Math.Ceiling(18 * scale); w.sizedOnce = true; w.idleMs = idle;
            return w;
        }
        public void SetClock(long t) { fakeNow = t; }

        long Now() { return preview ? fakeNow : Environment.TickCount; }

        public void Act(string k)
        {
            kind = k ?? "move";
            if (kind.StartsWith("key:")) { keyLabel = kind.Substring(4); kind = "key"; keyStart = Now(); }
            lastAct = Now();
            if (kind == "click") { bounceStart = Now(); if (Config.Current.CursorRipple) rippleStart = Now() + 30; }
            Appear();
        }

        public void SetPaused(bool p)
        {
            paused = p;
            lastAct = Now();
            if (p) Appear();
        }

        void Appear()
        {
            if (preview)
            {
                if (!pvVisible) { placed = false; curW = 0; lastStep = 0; appearStart = Now(); pvVisible = true; }
                return;
            }
            if (!IsHandleCreated) { var _ = Handle; }
            ApplyAffinity();
            if (!Visible) { placed = false; sizedOnce = false; curW = 0; lastStep = 0; appearStart = Now(); Show(); }
        }

        static float DpiScaleAt(int x, int y)
        {
            try
            {
                IntPtr mon = MonitorFromPoint(new Native.POINT { X = x, Y = y }, 2);
                uint dx, dy;
                if (GetDpiForMonitor(mon, 0, out dx, out dy) == 0 && dx > 0) return dx / 96f;
            }
            catch { }
            return 1f;
        }

        // Returns true while visible (the host keeps ticking ~60 fps to follow the cursor).
        public bool Step(int cx, int cy, int slot)
        {
            if (preview ? !pvVisible : !Visible) return false;
            long now = Now();
            double dt = lastStep == 0 ? 1 / 60.0 : Math.Max(0.001, Math.Min(0.1, (now - lastStep) / 1000.0));
            lastStep = now;
            if (paused) lastAct = now;
            long idle = now - lastAct;
            double fade = idle < idleMs ? 1.0 : 1.0 - (idle - idleMs) / (double)FadeMs;
            if (fade <= 0) { if (preview) pvVisible = false; else Hide(); placed = false; return false; }
            double fe = fade >= 1 ? 1 : fade * fade * (3 - 2 * fade);

            // Spring in: starts at 72%, overshoots a touch, settles.
            double t = (now - appearStart) / 1000.0;
            double spring = t >= 0.7 ? 1 : 1 - Math.Exp(-9 * t) * Math.Cos(14 * t);
            double scale = 0.72 + 0.28 * spring;
            double alpha = Math.Min(1.0, t / 0.12) * fe;
            if (fade < 1) scale *= 0.9 + 0.1 * fe;
            double bt = (now - bounceStart) / 1000.0;   // click bounce
            if (bt >= 0 && bt < 0.55) scale *= 1 - 0.08 * Math.Exp(-7 * bt) * Math.Sin(20 * bt);

            float follow = (float)(1 - Math.Exp(-dt * 20));
            if (!placed) { pos = new PointF(cx, cy); placed = true; }
            else pos = new PointF(pos.X + (cx - pos.X) * follow, pos.Y + (cy - pos.Y) * follow);
            if (Math.Abs(pos.X - cx) < 0.5f && Math.Abs(pos.Y - cy) < 0.5f) pos = new PointF(cx, cy);

            float ns = preview ? s : DpiScaleAt(cx, cy);
            if (ns != s || !sizedOnce) { s = ns; margin = (int)Math.Ceiling(18 * s); sizedOnce = true; curW = 0; }

            Render(now, dt, (now - rippleStart) / (double)RippleMs, slot, scale);
            int x = (int)Math.Round(pos.X) - margin, y = (int)Math.Round(pos.Y) - margin;
            if (preview)
            {
                PvX = x; PvY = y; PvAlpha = (byte)Math.Max(0, Math.Min(255, alpha * 255));
                return true;
            }
            var mi = new MONITORINFO { cbSize = Marshal.SizeOf(typeof(MONITORINFO)) };
            if (GetMonitorInfo(MonitorFromPoint(new Native.POINT { X = cx, Y = cy }, 2), ref mi))
            {
                if (x + frame.Width > mi.rcWork.Right) x = mi.rcWork.Right - frame.Width;
                if (y + frame.Height > mi.rcWork.Bottom) y = mi.rcWork.Bottom - frame.Height;
            }
            Blit(x, y, (byte)Math.Max(0, Math.Min(255, alpha * 255)));
            return true;
        }

        static System.Drawing.Drawing2D.GraphicsPath RoundRect(RectangleF r, float rad)
        {
            var p = new System.Drawing.Drawing2D.GraphicsPath();
            float d = rad * 2;
            p.AddArc(r.X, r.Y, d, d, 180, 90);
            p.AddArc(r.Right - d, r.Y, d, d, 270, 90);
            p.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
            p.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
            p.CloseFigure();
            return p;
        }

        static System.Drawing.Drawing2D.GraphicsPath Capsule(RectangleF r)
        {
            var p = new System.Drawing.Drawing2D.GraphicsPath();
            float d = r.Height;
            p.AddArc(r.X, r.Y, d, d, 90, 180);
            p.AddArc(r.Right - d, r.Y, d, d, 270, 180);
            p.CloseFigure();
            return p;
        }

        Font f1, f2, fk;
        float fontScale, curW;
        string keyLabel = "";
        long keyStart;
        long lastStep, bounceStart = -100000;

        void Render(long now, double dt, double ripple, int slot, double scale)
        {
            bool typing = kind == "type" && !paused;
            string secondary = paused ? "paused, you're using the PC" : typing ? "typing" : null;
            if (fontScale != s || f1 == null)
            {
                if (f1 != null) { f1.Dispose(); f2.Dispose(); fk.Dispose(); }
                fk = new Font(nameFamily, 12.5f * s, nameStyle, GraphicsUnit.Pixel);
                f1 = new Font(nameFamily, 15.5f * s, nameStyle, GraphicsUnit.Pixel);
                f2 = new Font(subFamily, 13.5f * s, FontStyle.Regular, GraphicsUnit.Pixel);
                fontScale = s;
            }
            using (var sf = (StringFormat)StringFormat.GenericTypographic.Clone())
            using (var mg = Graphics.FromHwnd(IntPtr.Zero))
            {
                sf.FormatFlags |= StringFormatFlags.MeasureTrailingSpaces | StringFormatFlags.NoWrap;
                mg.TextRenderingHint = System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
                SizeF t1 = mg.MeasureString(agent, f1, 1000, sf);
                SizeF t2 = secondary == null ? SizeF.Empty : mg.MeasureString(secondary, f2, 1000, sf);
                // Key chord shown as little keycaps: [Ctrl] + [Shift] + [T]
                string[] caps = kind == "key" && !paused && keyLabel.Length > 0 ? keyLabel.Split(new[] { " + " }, StringSplitOptions.None) : null;
                float capPad = 6 * s, capH = 21 * s, plusW = 0, capsW = 0;
                float[] capW = null;
                if (caps != null)
                {
                    capW = new float[caps.Length];
                    plusW = mg.MeasureString("+", f2, 1000, sf).Width + 8 * s;
                    for (int i = 0; i < caps.Length; i++)
                    {
                        capW[i] = Math.Max(capH, mg.MeasureString(caps[i], fk, 1000, sf).Width + capPad * 2);
                        capsW += capW[i] + (i > 0 ? plusW : 0);
                    }
                }

                float h = (float)Math.Round(36 * s), logo = 26 * s, padL = (h - logo) / 2, gap = 9 * s, gap2 = 7 * s, padR = 16 * s;
                float dotsW = typing ? 5 * s + 16 * s : 0;
                float target = padL + logo + gap + t1.Width + (secondary == null ? 0 : gap2 + t2.Width) + dotsW + (caps == null ? 0 : 10 * s + capsW) + padR;
                if (caps != null) target -= 6 * s;
                // Smooth width morph when the text changes.
                if (curW <= 0) curW = target;
                else
                {
                    curW += (target - curW) * (float)(1 - Math.Exp(-dt * 16));
                    if (Math.Abs(curW - target) < 0.5f) curW = target;
                }
                float w = curW;
                float px = margin + 14 * s, py = margin + 20 * s + slot * (h + 10 * s);
                float pad = 22 * s;
                int fw = (int)Math.Ceiling(px + Math.Max(w, target) * 1.06f + pad), fh = (int)Math.Ceiling(py + h * 1.06f + pad);
                if (frame == null || frame.Width < fw || frame.Height < fh || frame.Width > fw + 60 * s)
                {
                    if (frame != null) frame.Dispose();
                    frame = new Bitmap(fw + (int)(24 * s), fh, PixelFormat.Format32bppPArgb);
                }
                using (var g = Graphics.FromImage(frame))
                {
                    g.Clear(Color.Transparent);
                    g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.HighQuality;
                    g.PixelOffsetMode = System.Drawing.Drawing2D.PixelOffsetMode.HighQuality;
                    g.CompositingQuality = System.Drawing.Drawing2D.CompositingQuality.HighQuality;
                    g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic;
                    g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;

                    // Subtle click ripple around the real cursor tip (hollow, the tip stays visible).
                    if (ripple >= 0 && ripple <= 1)
                    {
                        double er = 1 - Math.Pow(1 - ripple, 3);
                        float r = (float)(4 * s + 14 * s * er);
                        using (var pen = new Pen(Color.FromArgb((int)(150 * (1 - ripple)), Ring), 2f * s))
                            g.DrawEllipse(pen, margin - r, margin - r, r * 2, r * 2);
                    }

                    g.TranslateTransform(px, py + h / 2);
                    g.ScaleTransform((float)scale, (float)scale);
                    g.TranslateTransform(-px, -(py + h / 2));

                    var rect = new RectangleF(px, py, w, h);
                    // Soft drop shadow.
                    for (int i = 8; i >= 1; i--)
                    {
                        float e = i * 1.25f * s;
                        using (var sp = Capsule(new RectangleF(rect.X - e, rect.Y - e + 3f * s, rect.Width + 2 * e, rect.Height + 2 * e)))
                        using (var br = new SolidBrush(Color.FromArgb((int)(4 + (8 - i) * 2.1), 15, 23, 42)))
                            g.FillPath(br, sp);
                    }
                    using (var path = Capsule(rect))
                    {
                        using (var br = new System.Drawing.Drawing2D.LinearGradientBrush(new RectangleF(rect.X, rect.Y - 1, rect.Width, rect.Height + 2),
                            paused ? BgPaused : Bg, paused ? BgPaused2 : Bg2, 90f))
                            g.FillPath(br, path);

                        var clip = g.Clip;
                        g.SetClip(path, System.Drawing.Drawing2D.CombineMode.Intersect);
                        // Shimmer sweep while working.
                        if (!paused && !typing && kind != "key")
                        {
                            double sp = (now % 2400) / 2400.0;
                            float band = 46 * s, bx = (float)(rect.X - band * 2 + (rect.Width + band * 3) * sp);
                            using (var sh = new System.Drawing.Drawing2D.LinearGradientBrush(new RectangleF(bx, rect.Y, band, rect.Height),
                                Color.FromArgb(0, 255, 255, 255), Color.FromArgb(0, 255, 255, 255), 20f))
                            {
                                var cb = new System.Drawing.Drawing2D.ColorBlend
                                {
                                    Colors = new[] { Color.FromArgb(0, 210, 225, 255), Color.FromArgb(120, 214, 228, 255), Color.FromArgb(0, 210, 225, 255) },
                                    Positions = new[] { 0f, 0.5f, 1f }
                                };
                                sh.InterpolationColors = cb;
                                g.FillRectangle(sh, bx, rect.Y, band, rect.Height);
                            }
                        }
                        // top sheen
                        using (var sheen = new System.Drawing.Drawing2D.LinearGradientBrush(new RectangleF(rect.X, rect.Y, rect.Width, rect.Height * 0.55f),
                            Color.FromArgb(150, 255, 255, 255), Color.FromArgb(0, 255, 255, 255), 90f))
                            g.FillRectangle(sheen, rect.X, rect.Y, rect.Width, rect.Height * 0.55f - 0.5f);

                        // Logo, with a soft pulse ring.
                        float lx = rect.X + padL, ly = rect.Y + padL;
                        double pp = (now % (paused ? 1800 : 1400)) / (double)(paused ? 1800 : 1400);
                        if (!typing)
                        {
                            float ex = (float)(5 * s * pp);
                            using (var br = new SolidBrush(Color.FromArgb((int)(90 * (1 - pp) * (1 - pp)), paused ? Amber : Work)))
                                g.FillEllipse(br, lx - ex, ly - ex, logo + 2 * ex, logo + 2 * ex);
                        }
                        ReachLogo.Draw(g, new RectangleF(lx, ly, logo, logo), paused);

                        // Text.
                        float tx = lx + logo + gap;
                        using (var b1 = new SolidBrush(Ink))
                            g.DrawString(agent, f1, b1, tx, rect.Y + (h - t1.Height) / 2 - 0.5f * s, sf);
                        float ex2 = tx + t1.Width + gap2;
                        if (secondary != null)
                            using (var b2 = new SolidBrush(paused ? Ink2Paused : Ink2))
                                g.DrawString(secondary, f2, b2, ex2, rect.Y + (h - t2.Height) / 2 - 0.2f * s, sf);
                        if (caps != null)
                        {
                            float kx = tx + t1.Width + 10 * s, ky = rect.Y + (h - capH) / 2;
                            double kp = Math.Min(1, (now - keyStart) / 260.0);       // keycaps pop in
                            for (int i = 0; i < caps.Length; i++)
                            {
                                if (i > 0)
                                {
                                    using (var bp = new SolidBrush(Ink2))
                                        g.DrawString("+", f2, bp, kx + 4 * s, rect.Y + (h - t1.Height * 0.86f) / 2, sf);
                                    kx += plusW;
                                }
                                double ci = Math.Max(0, Math.Min(1, kp * caps.Length - i * 0.6));
                                float press = (float)(1.5 * s * (1 - ci));
                                var kr = new RectangleF(kx, ky + press, capW[i], capH);
                                using (var kpth = RoundRect(new RectangleF(kr.X, kr.Y + 1.5f * s, kr.Width, kr.Height), 6 * s))
                                using (var kb = new SolidBrush(Color.FromArgb(30, 15, 23, 42))) g.FillPath(kb, kpth);
                                using (var kpth = RoundRect(kr, 6 * s))
                                {
                                    using (var kb = new System.Drawing.Drawing2D.LinearGradientBrush(kr, Color.White, Color.FromArgb(255, 238, 241, 248), 90f)) g.FillPath(kb, kpth);
                                    using (var kpen = new Pen(Color.FromArgb(46, 15, 23, 42), 1f * s)) g.DrawPath(kpen, kpth);
                                }
                                using (var tb = new SolidBrush(Ink))
                                using (var cf = new StringFormat(sf) { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center })
                                    g.DrawString(caps[i], fk, tb, new RectangleF(kr.X, kr.Y - 0.5f * s, kr.Width, kr.Height), cf);
                                kx += capW[i];
                            }
                        }
                        // Typing dots.
                        if (typing)
                        {
                            float dx = ex2 + t2.Width + 5 * s, dy = rect.Y + h / 2 + 1.5f * s, r = 2.1f * s;
                            for (int i = 0; i < 3; i++)
                            {
                                double ph = Math.Sin(now / 1000.0 * Math.PI * 2 / 0.9 - i * 0.7);
                                double up = Math.Max(0, ph);
                                using (var br = new SolidBrush(Color.FromArgb((int)(110 + 145 * up), Typing)))
                                    g.FillEllipse(br, dx + i * 6 * s - r, (float)(dy - 3 * s * up) - r, r * 2, r * 2);
                            }
                        }
                        g.Clip = clip;
                        using (var pen = new Pen(paused ? BorderPaused : Border, 1f * s)) g.DrawPath(pen, path);
                    }
                }
            }
            if (hbmp != IntPtr.Zero) { DeleteObject(hbmp); hbmp = IntPtr.Zero; }
        }

        void Blit(int x, int y, byte alpha)
        {
            // Format32bppPArgb -> premultiplied DIB for UpdateLayeredWindow.
            if (hbmp == IntPtr.Zero) hbmp = frame.GetHbitmap(Color.FromArgb(0));
            IntPtr screenDc = GetDC(IntPtr.Zero);
            IntPtr memDc = CreateCompatibleDC(screenDc);
            IntPtr old = SelectObject(memDc, hbmp);
            try
            {
                var size = new SIZE { cx = frame.Width, cy = frame.Height };
                var src = new Native.POINT { X = 0, Y = 0 };
                var dst = new Native.POINT { X = x, Y = y };
                var blend = new BLENDFUNCTION { BlendOp = 0, BlendFlags = 0, SourceConstantAlpha = alpha, AlphaFormat = 1 };
                UpdateLayeredWindow(Handle, screenDc, ref dst, ref size, memDc, ref src, 0, ref blend, 2 /*ULW_ALPHA*/);
                SetWindowPos(Handle, new IntPtr(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010 | 0x0200); // topmost, no owner z-change
            }
            finally
            {
                SelectObject(memDc, old);
                DeleteDC(memDc);
                ReleaseDC(IntPtr.Zero, screenDc);
            }
        }

        [StructLayout(LayoutKind.Sequential)] struct SIZE { public int cx, cy; }
        [StructLayout(LayoutKind.Sequential, Pack = 1)] struct BLENDFUNCTION { public byte BlendOp, BlendFlags, SourceConstantAlpha, AlphaFormat; }
        [DllImport("user32.dll")] static extern bool UpdateLayeredWindow(IntPtr h, IntPtr hdcDst, ref Native.POINT pptDst, ref SIZE psize, IntPtr hdcSrc, ref Native.POINT pptSrc, int key, ref BLENDFUNCTION blend, int flags);
        [DllImport("user32.dll")] static extern bool SetWindowDisplayAffinity(IntPtr h, uint aff);
        [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint f);
        [DllImport("user32.dll")] static extern IntPtr MonitorFromPoint(Native.POINT pt, uint flags);
        [StructLayout(LayoutKind.Sequential)] struct MONITORINFO { public int cbSize; public Native.RECT rcMonitor, rcWork; public uint dwFlags; }
        [DllImport("user32.dll")] static extern bool GetMonitorInfo(IntPtr mon, ref MONITORINFO mi);
        [DllImport("shcore.dll")] static extern int GetDpiForMonitor(IntPtr mon, int type, out uint dx, out uint dy);
        [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
        [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h, IntPtr dc);
        [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr dc);
        [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr dc);
        [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc, IntPtr o);
        [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);
    }
}

// The Reach mark: a soft gradient circle with a white pointer reaching up-left.
static class ReachLogo
{
    public static void Draw(Graphics g, RectangleF r, bool paused)
    {
        var oldMode = g.SmoothingMode;
        g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.HighQuality;
        Color a = paused ? Color.FromArgb(255, 252, 196, 74) : Color.FromArgb(255, 99, 140, 255);
        Color b = paused ? Color.FromArgb(255, 234, 128, 12) : Color.FromArgb(255, 124, 77, 246);
        using (var br = new System.Drawing.Drawing2D.LinearGradientBrush(r, a, b, 45f)) g.FillEllipse(br, r);
        // inner glow at the top
        using (var hp = new System.Drawing.Drawing2D.GraphicsPath())
        {
            hp.AddEllipse(r.X + r.Width * 0.12f, r.Y + r.Height * 0.04f, r.Width * 0.76f, r.Height * 0.5f);
            using (var hb = new System.Drawing.Drawing2D.PathGradientBrush(hp))
            {
                hb.CenterColor = Color.FromArgb(70, 255, 255, 255);
                hb.SurroundColors = new[] { Color.FromArgb(0, 255, 255, 255) };
                g.FillPath(hb, hp);
            }
        }
        // pointer arrow, 12.5 x 18.5 units, centered slightly right/down of center
        float u = r.Height * 0.54f / 18.5f;
        float ox = r.X + r.Width * 0.5f - 12.5f * u * 0.40f, oy = r.Y + r.Height * 0.5f - 18.5f * u * 0.50f;
        var pts = new[] { new PointF(0, 0), new PointF(0, 15.5f), new PointF(4.2f, 11.8f), new PointF(7.1f, 18.2f), new PointF(9.9f, 17f), new PointF(7.1f, 10.8f), new PointF(12.5f, 10.8f) };
        for (int i = 0; i < pts.Length; i++) pts[i] = new PointF(ox + pts[i].X * u, oy + pts[i].Y * u);
        using (var ap = new System.Drawing.Drawing2D.GraphicsPath())
        {
            ap.AddPolygon(pts);
            using (var sh = new SolidBrush(Color.FromArgb(50, 20, 10, 60)))
            {
                var m = new System.Drawing.Drawing2D.Matrix(); m.Translate(0, r.Height * 0.035f);
                using (var sp = (System.Drawing.Drawing2D.GraphicsPath)ap.Clone()) { sp.Transform(m); g.FillPath(sh, sp); }
            }
            g.FillPath(Brushes.White, ap);
            using (var pen = new Pen(Color.White, Math.Max(1f, r.Height * 0.05f)) { LineJoin = System.Drawing.Drawing2D.LineJoin.Round })
                g.DrawPath(pen, ap);
        }
        g.SmoothingMode = oldMode;
    }

    public static Bitmap Render(int size, bool paused)
    {
        var bmp = new Bitmap(size, size, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(bmp))
        {
            g.Clear(Color.Transparent);
            g.PixelOffsetMode = System.Drawing.Drawing2D.PixelOffsetMode.HighQuality;
            float m = Math.Max(1, size / 32f);
            Draw(g, new RectangleF(m, m, size - 2 * m, size - 2 * m), paused);
        }
        return bmp;
    }
}

// ---------------------------------------------------------------------------
// Pause-on-user: a passive low-level keyboard + mouse hook (never blocks, always
// calls the next hook) notes the last physical (non-injected) input. Before any
// action that injects input, Reach waits until the user has been idle for
// userPauseIdleMs, and fails the command after userPauseTimeoutMs.
// Input dropped by an input-lock tool (inputlock.exe) is ignored while it runs.
// ---------------------------------------------------------------------------
static class UserYield
{
    delegate IntPtr HookProc(int n, IntPtr w, IntPtr l);
    [DllImport("user32.dll", SetLastError = true)] static extern IntPtr SetWindowsHookEx(int id, HookProc fn, IntPtr mod, uint tid);
    [DllImport("user32.dll")] static extern IntPtr CallNextHookEx(IntPtr h, int n, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern int GetMessage(out MSGx m, IntPtr h, uint a, uint b);
    [DllImport("kernel32.dll")] static extern IntPtr GetModuleHandle(string n);
    [StructLayout(LayoutKind.Sequential)] struct MSGx { public IntPtr h; public uint m; public IntPtr w, l; public uint t; public int x, y; }

    static HookProc kp, mp;
    static IntPtr kh, mh;
    static int lastPhysical = Environment.TickCount - 3600000;
    static int simulateUntil = Environment.TickCount - 1;
    static volatile bool inputLockRunning;
    static int waiting;
    public static event Action<bool> StateChanged;   // true = paused for the user

    public static bool HooksOk { get { return kh != IntPtr.Zero && mh != IntPtr.Zero; } }
    public static bool InputLockRunning { get { return inputLockRunning; } }

    public static void Start()
    {
        var t = new Thread(delegate ()
        {
            kp = K; mp = M;
            IntPtr mod = GetModuleHandle(null);
            kh = SetWindowsHookEx(13, kp, mod, 0);
            mh = SetWindowsHookEx(14, mp, mod, 0);
            Log.Write("pause-on-user hooks: keyboard " + (kh != IntPtr.Zero) + ", mouse " + (mh != IntPtr.Zero));
            MSGx m;
            while (GetMessage(out m, IntPtr.Zero, 0, 0) > 0) { }
        });
        t.IsBackground = true;
        t.Priority = ThreadPriority.Highest;
        t.Name = "reach-user-hooks";
        t.Start();

        var w = new Thread(delegate ()
        {
            while (true)
            {
                try
                {
                    var ps = Process.GetProcessesByName("inputlock");
                    inputLockRunning = ps.Length > 0;
                    foreach (var p in ps) p.Dispose();
                }
                catch { }
                Thread.Sleep(250);
            }
        });
        w.IsBackground = true;
        w.Name = "reach-inputlock-watch";
        w.Start();
    }

    // Passive: record, never swallow.
    static IntPtr K(int n, IntPtr w, IntPtr l)
    {
        if (n >= 0 && !inputLockRunning && (Marshal.ReadInt32(l, 8) & 0x12) == 0) // LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED
            lastPhysical = Environment.TickCount;
        return CallNextHookEx(kh, n, w, l);
    }

    static IntPtr M(int n, IntPtr w, IntPtr l)
    {
        if (n >= 0 && !inputLockRunning && (Marshal.ReadInt32(l, 12) & 0x3) == 0) // LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED
            lastPhysical = Environment.TickCount;
        return CallNextHookEx(mh, n, w, l);
    }

    public static int IdleMs
    {
        get
        {
            int now = Environment.TickCount;
            if (unchecked(simulateUntil - now) > 0) return 0;
            return unchecked(now - lastPhysical);
        }
    }

    // Test hook: pretend the user is using the PC for ms.
    public static void Simulate(int ms)
    {
        simulateUntil = Environment.TickCount + Math.Max(0, Math.Min(ms, 120000));
        lastPhysical = simulateUntil;
    }

    public static bool Gated(string tool, string action)
    {
        tool = (tool ?? "").ToLowerInvariant();
        action = (action ?? "").ToLowerInvariant();
        switch (tool)
        {
            case "mouse": return action != "position" && action != "cursor_position" && action != "wait";
            case "keyboard": return true;
            case "ui": return action == "invoke" || action == "click" || action == "set" || action == "set_value" || action == "toggle" || action == "focus";
            case "apps": return action == "open" || action == "launch" || action == "start" || action == "focus" || action == "close"
                             || action == "minimize" || action == "maximize" || action == "restore";
            default: return false;
        }
    }

    // Blocks until the user has been idle long enough. Returns ms waited.
    // Throws TimeoutException with a clear message when the pause runs too long.
    public static int WaitForIdle(string agent)
    {
        if (!Config.Current.PauseOnUser) return 0;
        int idleNeed = Math.Max(100, Config.Current.UserPauseIdleMs);
        if (IdleMs >= idleNeed) return 0;
        int timeout = Math.Max(1000, Config.Current.UserPauseTimeoutMs);
        var sw = Stopwatch.StartNew();
        bool first = Interlocked.Increment(ref waiting) == 1;
        if (first) Raise(true);
        AgentCursor.Paused(agent, true);
        try
        {
            while (IdleMs < idleNeed)
            {
                if (sw.ElapsedMilliseconds > timeout)
                    throw new TimeoutException("Paused because you're using the PC: no " + (idleNeed / 1000.0).ToString("0.#")
                        + " s idle gap within " + (timeout / 1000.0).ToString("0.#") + " s, so the command was not run. Retry when the PC is idle.");
                Thread.Sleep(40);
            }
            return (int)sw.ElapsedMilliseconds;
        }
        finally
        {
            AgentCursor.Paused(agent, false);
            if (Interlocked.Decrement(ref waiting) == 0) Raise(false);
        }
    }

    static void Raise(bool paused)
    {
        var h = StateChanged;
        if (h != null) try { h(paused); } catch { }
    }
}

static class Program
{
    static readonly string MutexName = "Local\\ReachDesktopControl.1"
        + (string.IsNullOrEmpty(Environment.GetEnvironmentVariable("REACH_HOME")) ? "" : "." + (uint)Paths.Dir.ToLowerInvariant().GetHashCode());

    [STAThread]
    static int Main(string[] args)
    {
        try { Native.SetProcessDPIAware(); } catch { }
        try { WebRequest.DefaultWebProxy = null; } catch { }
        RebindConsole();
        Directory.CreateDirectory(Paths.Dir);
        Config.Load();

        if (args.Length >= 1 && args[0].Equals("selftest", StringComparison.OrdinalIgnoreCase))
        {
            string r = Engine.SelfTest();
            Console.WriteLine(r);
            return r.Contains("FAIL") ? 1 : 0;
        }

        if (args.Length >= 1 && args[0].Equals("pill-preview", StringComparison.OrdinalIgnoreCase))
        {
            string dir = Path.Combine(Paths.Dir, "pill-preview"), agent = "Meridian";
            float sc = 2f;
            for (int i = 1; i < args.Length; i++)
            {
                if (args[i] == "--out" && i + 1 < args.Length) dir = args[++i];
                else if (args[i] == "--agent" && i + 1 < args.Length) agent = args[++i];
                else if (args[i] == "--scale" && i + 1 < args.Length) sc = float.Parse(args[++i], System.Globalization.CultureInfo.InvariantCulture);
            }
            Console.WriteLine(AgentCursor.Preview(dir, agent, sc));
            return 0;
        }

        if (args.Length >= 2 && args[0].Equals("logo", StringComparison.OrdinalIgnoreCase))
        {
            using (var bmp = ReachLogo.Render(args.Length > 2 ? int.Parse(args[2]) : 256, false)) bmp.Save(args[1], ImageFormat.Png);
            Console.WriteLine("wrote " + args[1]);
            return 0;
        }

        if (args.Length == 0)
        {
            // Startup shortcut and double-click: run the tray server.
            // If a server is already up, just print status (when a console is attached).
            string t;
            if (Cli.Health(out t))
            {
                try
                {
                    Cli.EnsureServer();
                    Console.WriteLine(t);
                }
                catch { }
                return 0;
            }
            return Serve();
        }

        try
        {
            if (args[0].Equals("serve", StringComparison.OrdinalIgnoreCase)
                || args[0].Equals("start", StringComparison.OrdinalIgnoreCase)
                || args[0].Equals("mcp", StringComparison.OrdinalIgnoreCase))
                return Serve();
            return Cli.Run(args);
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine(ex.Message);
            Log.Write("cli: " + ex);
            return 1;
        }
    }

    static void RebindConsole()
    {
        try
        {
            if (!Native.AttachConsole(-1)) return;
            var stdout = new StreamWriter(Console.OpenStandardOutput()) { AutoFlush = true };
            Console.SetOut(stdout);
            Console.SetError(new StreamWriter(Console.OpenStandardError()) { AutoFlush = true });
        }
        catch { }
    }

    public static int Serve()
    {
        bool created;
        var mutex = new Mutex(true, MutexName, out created);
        if (!created)
        {
            Console.WriteLine("Reach is already running.");
            return 0;
        }
        try
        {
            IntPtr cw = Native.GetConsoleWindow();
            if (cw != IntPtr.Zero) Native.ShowWindow(cw, 0); // SW_HIDE
            Native.FreeConsole();
        }
        catch { }
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        var ctx = new WindowsFormsSynchronizationContext();
        SynchronizationContext.SetSynchronizationContext(ctx);
        var engine = new Engine(SynchronizationContext.Current);
        try { UserYield.Start(); } catch (Exception ex) { Log.Write("pause-on-user: " + ex.Message); }
        var server = new Server(engine);
        try { server.Start(); }
        catch (Exception ex)
        {
            MessageBox.Show("Reach could not start its server: " + ex.Message, "Reach");
            return 1;
        }
        var tray = new Tray(engine, server);
        Application.Run(tray);
        try { mutex.ReleaseMutex(); } catch { }
        return 0;
    }
}
