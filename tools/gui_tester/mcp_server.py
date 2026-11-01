"""
GUI Tester MCP server.

Gives an AI agent (Claude Code, Cursor, Antigravity, ...) eyes and hands for a desktop GUI app
(GPUI / Slint / iced / anything GPU-rendered that has no accessibility tree):

  * launch / stop the app, read its stdout+stderr log (Rust panics!)
  * screenshots of the app window - returned as real images the model can SEE
  * clicks / hover / drag / scroll / typing, all in WINDOW-RELATIVE coordinates
    (stays correct when the window moves)
  * find elements by template image, multi-scale (DPI tolerant), or by text (OCR, optional)
  * wait_for_change / wait_until_stable instead of blind sleeps
  * baseline screenshots + visual diff for regression testing
  * run_steps: execute many actions in one call

Run (stdio transport):   python tools/gui_tester/mcp_server.py
IMPORTANT: stdout is the protocol channel - never print() to stdout in this file.
"""

import atexit
import io
import json
import os
import re
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Optional

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent.parent))

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFont
try:  # mcp >= 2.0
    from mcp.server.mcpserver import MCPServer as FastMCP, Image as McpImage
except ImportError:  # mcp 1.x
    from mcp.server.fastmcp import FastMCP, Image as McpImage

from tools.gui_tester.core import (  # noqa: E402  (core must be imported before pyautogui: DPI awareness)
    AppSession,
    Clipboard,
    ImageSearch,
    InputController,
    ScreenCapture,
    VisualDiff,
    WindowInfo,
    WindowManager,
)

SHOTS_DIR = HERE / "screenshots"
TEMPLATES_DIR = HERE / "templates"
BASELINES_DIR = HERE / "baselines"
LOGS_DIR = HERE / "logs"
for _d in (SHOTS_DIR, TEMPLATES_DIR, BASELINES_DIR, LOGS_DIR):
    _d.mkdir(parents=True, exist_ok=True)

INSTRUCTIONS = """\
Tools to run and visually test a desktop GUI app on this Windows machine.

Workflow: launch_app (or set_default_window if already running) -> screenshot -> act -> verify.
* All x/y coordinates are WINDOW-RELATIVE pixels (0,0 = top-left of the app window), the same
  coordinates you read from screenshot(). Use screenshot(grid=100) to read coordinates reliably.
* After any action that changes the UI, prefer wait_for_change / wait_until_stable over sleeping,
  then screenshot (or pass screenshot_after=true to the action).
* Prefer click_text (OCR) or click_template over guessing pixel coordinates. Create templates with
  save_template(name, x, y, w, h) from the current window.
* If the app misbehaves or disappears, call app_logs - Rust panics and stderr are captured there.
* Use run_steps to batch several actions into one call.
* Real mouse/keyboard are used: don't touch the mouse while a test runs; the window must be visible.
"""

mcp = FastMCP("gui-tester", instructions=INSTRUCTIONS)

# --------------------------------------------------------------------------------------
# State & helpers
# --------------------------------------------------------------------------------------

_sessions: dict = {}
_default_window: Optional[str] = os.environ.get("GUI_TESTER_WINDOW") or None
_pg_ready = False


def _pg():
    """Lazy pyautogui import + config."""
    global _pg_ready
    import pyautogui

    if not _pg_ready:
        pyautogui.PAUSE = 0.03
        pyautogui.FAILSAFE = True  # slam the mouse into a screen corner to abort a runaway test
        _pg_ready = True
    return pyautogui


def _session_pids() -> set:
    pids: set = set()
    for s in _sessions.values():
        pids |= s.pids()
    return pids


def _win(window: Optional[str] = None) -> WindowInfo:
    title = window or _default_window
    if not title:
        raise ValueError(
            "No window specified. Pass window='<title substring>', call set_default_window, "
            "or launch_app(window_title=...). list_windows shows what is open."
        )
    w = None
    for s in _sessions.values():
        if s.is_running():
            # only fall back to "any window of the launched app" for the default window
            w = s.find_window(title, allow_any_owned=(title == _default_window))
            if w:
                break
    if not w:
        w = WindowManager.find_window(title, pids=_session_pids() or None)
    if not w:
        raise ValueError(
            f"Window matching '{title}' not found. Use list_windows, or app_windows to see every window "
            "(even hidden/untitled) owned by the launched app."
        )
    return w


def _prep(window: Optional[str] = None, focus: bool = True) -> WindowInfo:
    """Resolve window, bring to front, re-read geometry."""
    w = _win(window)
    if focus:
        WindowManager.focus(w.hwnd)
        time.sleep(0.08)
        w = WindowManager.get_window_info(w.hwnd) or w
    if w.is_minimized:
        raise ValueError("Window is minimized and could not be restored.")
    return w


def _grab(w: WindowInfo) -> Image.Image:
    w = WindowManager.get_window_info(w.hwnd) or w
    img = ScreenCapture.capture_window_hwnd(w.hwnd)
    if img is not None:
        return img
    return ScreenCapture.capture_rect(w.x, w.y, w.width, w.height).convert("RGB")


def _to_bgr(img: Image.Image) -> np.ndarray:
    return cv2.cvtColor(np.array(img.convert("RGB")), cv2.COLOR_RGB2BGR)


def _check_xy(w: WindowInfo, x: int, y: int):
    if not (0 <= x < w.width and 0 <= y < w.height):
        raise ValueError(f"({x},{y}) is outside the window ({w.width}x{w.height}). Coordinates are window-relative.")


def _abs(w: WindowInfo, x: int, y: int):
    _check_xy(w, x, y)
    return w.x + int(x), w.y + int(y)


def _resolve_template(name: str) -> Path:
    for cand in (Path(name), TEMPLATES_DIR / name, TEMPLATES_DIR / f"{name}.png"):
        if cand.is_file():
            return cand
    avail = sorted(p.stem for p in TEMPLATES_DIR.glob("*.png"))
    raise ValueError(f"Template '{name}' not found. Available templates: {avail}. Create one with save_template.")


def _safe_name(name: str) -> str:
    n = re.sub(r"[^A-Za-z0-9_.-]+", "_", name).strip("._")
    if not n:
        raise ValueError("Invalid name")
    return n


def _prune_shots(keep: int = 150):
    files = sorted(SHOTS_DIR.glob("auto_*.png"), key=lambda p: p.stat().st_mtime)
    for p in files[:-keep]:
        try:
            p.unlink()
        except OSError:
            pass


def _draw_grid(img: Image.Image, step: int) -> Image.Image:
    img = img.copy()
    d = ImageDraw.Draw(img, "RGBA")
    try:
        font = ImageFont.load_default(size=11)
    except TypeError:
        font = ImageFont.load_default()
    for x in range(0, img.width, step):
        d.line([(x, 0), (x, img.height)], fill=(255, 0, 255, 90), width=1)
    for y in range(0, img.height, step):
        d.line([(0, y), (img.width, y)], fill=(255, 0, 255, 90), width=1)
    for x in range(0, img.width, step):
        for y in range(0, img.height, step):
            label = f"{x},{y}"
            d.rectangle([x + 1, y + 1, x + 1 + 6 * len(label), y + 13], fill=(0, 0, 0, 150))
            d.text((x + 2, y + 1), label, fill=(255, 255, 0, 255), font=font)
    return img


def _render(img: Image.Image, grid: int = 0, max_width: int = 1600):
    """PIL image -> MCP image (optionally gridded and downscaled) + scale factor."""
    W, H = img.size
    view = img.convert("RGB")
    if grid and grid > 0:
        view = _draw_grid(view, int(grid))
    scale = 1.0
    if max_width and W > max_width:
        scale = max_width / W
        view = view.resize((max_width, max(1, int(H * scale))), Image.LANCZOS)
    buf = io.BytesIO()
    view.save(buf, "PNG")
    return McpImage(data=buf.getvalue(), format="png"), scale


def _dump(data: dict) -> str:
    return json.dumps(data, ensure_ascii=False, default=str)


def _done(info: dict, w: WindowInfo, shot: bool, delay: float = 0.3, grid: int = 0, max_width: int = 1600):
    """Common tail of an action: wait a bit, optionally attach a screenshot."""
    time.sleep(delay)
    info.setdefault("ok", True)
    if not shot:
        return _dump(info)
    img = _grab(w)
    mimg, scale = _render(img, grid, max_width)
    info["window_size"] = [img.width, img.height]
    if scale != 1.0:
        info["image_scale"] = round(scale, 4)
        info["note"] = "image is downscaled; coordinates you pass to tools are still in ORIGINAL window pixels"
    return [_dump(info), mimg]


def _text_of(result) -> str:
    if isinstance(result, list):
        return " ".join(r for r in result if isinstance(r, str))
    return str(result)


def _match(src: np.ndarray, tpl: np.ndarray, scales=(1.0,)) -> Optional[dict]:
    best = None
    for s in scales:
        t = tpl
        if s != 1.0:
            t = cv2.resize(tpl, None, fx=s, fy=s, interpolation=cv2.INTER_AREA if s < 1 else cv2.INTER_CUBIC)
        th, tw = t.shape[:2]
        if th < 4 or tw < 4 or th > src.shape[0] or tw > src.shape[1]:
            continue
        res = cv2.matchTemplate(src, t, cv2.TM_CCOEFF_NORMED)
        _, mx, _, loc = cv2.minMaxLoc(res)
        if not np.isfinite(mx):  # flat single-colour template
            mx = 0.0
        if best is None or mx > best["score"]:
            best = {"score": float(mx), "x": int(loc[0]), "y": int(loc[1]), "w": int(tw), "h": int(th), "scale": s}
    return best


SCALES_MULTI = (1.0, 0.9, 1.1, 0.8, 1.25, 0.75, 1.5, 0.67, 2.0, 0.5)


def _locate(w: WindowInfo, template: str, threshold: float, multi_scale: bool, timeout: float, present: bool = True):
    """Poll the window for a template. Returns match dict (window-relative) or None."""
    tpl = ImageSearch._to_cv2(_resolve_template(template))
    scales = SCALES_MULTI if multi_scale else (1.0,)
    deadline = time.time() + max(0.0, timeout)
    best_seen = None
    while True:
        m = _match(_to_bgr(_grab(w)), tpl, scales)
        if m:
            best_seen = m if (best_seen is None or m["score"] > best_seen["score"]) else best_seen
            hit = m["score"] >= threshold
            if hit == present:
                m["cx"], m["cy"] = m["x"] + m["w"] // 2, m["y"] + m["h"] // 2
                return m, best_seen
        if time.time() >= deadline:
            return None, best_seen
        time.sleep(0.25)


# ---- OCR (optional) -------------------------------------------------------------------

_ocr_engine = None


def _ocr_items(img: Image.Image) -> list:
    global _ocr_engine
    if _ocr_engine is None:
        try:
            from rapidocr_onnxruntime import RapidOCR
        except ImportError as e:
            raise RuntimeError("OCR is not installed. Run: pip install rapidocr-onnxruntime") from e
        _ocr_engine = RapidOCR()
    result, _ = _ocr_engine(_to_bgr(img))
    items = []
    for box, text, score in result or []:
        xs = [p[0] for p in box]
        ys = [p[1] for p in box]
        x, y = int(min(xs)), int(min(ys))
        w, h = int(max(xs) - x), int(max(ys) - y)
        items.append(
            {"text": text, "score": round(float(score), 3), "x": x, "y": y, "w": w, "h": h, "cx": x + w // 2, "cy": y + h // 2}
        )
    return items


def _find_text_items(img: Image.Image, text: str, exact: bool, case: bool) -> list:
    needle = text if case else text.lower()
    out = []
    for it in _ocr_items(img):
        hay = it["text"] if case else it["text"].lower()
        if (hay.strip() == needle.strip()) if exact else (needle in hay):
            out.append(it)
    out.sort(key=lambda i: (i["y"], i["x"]))
    return out


def _ratio(a: Image.Image, b: Image.Image, pixel_threshold: int = 16) -> float:
    A = np.asarray(a.convert("L"), dtype=np.int16)
    B = np.asarray(b.convert("L"), dtype=np.int16)
    if A.shape != B.shape:
        return 1.0
    return float((np.abs(A - B) > pixel_threshold).mean())


# --------------------------------------------------------------------------------------
# App lifecycle
# --------------------------------------------------------------------------------------


@mcp.tool()
def launch_app(
    cmd: str,
    cwd: Optional[str] = None,
    window_title: Optional[str] = None,
    timeout: float = 20.0,
    name: str = "main",
    screenshot: bool = True,
):
    """Start an application and wait for its window.

    cmd: executable path or shell command (e.g. 'aquilum-gpui/target/debug/vibecodium.exe' or 'cargo run').
    window_title: substring of the window title to wait for; it also becomes the default window.
    timeout: seconds to wait for the window (use 120+ for 'cargo run' that has to compile).
    name: session name, lets you run several apps (server + client) at once.
    stdout/stderr go to a log file readable via app_logs.
    """
    global _default_window
    old = _sessions.get(name)
    if old and old.is_running():
        return _dump({"ok": False, "error": f"Session '{name}' is already running (pid {old.process.pid}). Call stop_app first."})

    log = LOGS_DIR / f"{_safe_name(name)}.log"
    s = AppSession(cmd, working_dir=cwd, log_path=str(log))
    ok = s.start(wait_secs=2.0, window_title=window_title, timeout=timeout)
    _sessions[name] = s

    info = {"ok": ok, "name": name, "pid": s.process.pid if s.process else None, "log": str(log)}
    if not ok:
        info["exit_code"] = s.exit_code()
        info["error"] = "Window did not appear" if window_title else "Process is not running"
        info["log_tail"] = _tail(log, 25)
        info["process_alive"] = s.is_running()
        info["owned_windows"] = [w.to_dict() for w in s.owned_windows(include_hidden=True)]
        info["hint"] = (
            "If process_alive is true but owned_windows is empty, the app started without creating a window "
            "(see log_tail). If owned_windows lists a window, its title/class differ from window_title - "
            "call launch_app again without window_title, or use set_default_window with its exact title."
        )
        return _dump(info)

    if window_title:
        _default_window = window_title
        w = s.window_info
        info["window"] = w.to_dict() if w else None
        if screenshot and w:
            return _done(info, w, True, delay=0.5)
    return _dump(info)


@mcp.tool()
def stop_app(name: str = "main", force: bool = False):
    """Stop a launched application (kills the whole process tree, incl. children of `cargo run`)."""
    s = _sessions.pop(name, None)
    if not s:
        return _dump({"ok": False, "error": f"No session '{name}'. Running: {list(_sessions)}"})
    code = s.exit_code()
    s.stop(force=force)
    return _dump({"ok": True, "name": name, "exit_code_before_stop": code})


@mcp.tool()
def app_status():
    """List launched app sessions with their running state and exit codes."""
    return _dump(
        {
            n: {"running": s.is_running(), "pid": s.process.pid if s.process else None, "exit_code": s.exit_code(), "cmd": s.cmd}
            for n, s in _sessions.items()
        }
    )


def _tail(path: Path, lines: int) -> str:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""
    return "\n".join(text.splitlines()[-lines:])


@mcp.tool()
def app_logs(name: str = "main", tail_lines: int = 60):
    """Return the last lines of the app's stdout/stderr (panics, tracing output, build errors)."""
    log = LOGS_DIR / f"{_safe_name(name)}.log"
    if not log.exists():
        return "No log for this session."
    s = _sessions.get(name)
    head = f"[running={s.is_running() if s else 'unknown'} exit_code={s.exit_code() if s else 'n/a'}]\n"
    return head + _tail(log, max(1, tail_lines))


@mcp.tool()
def app_windows(name: str = "main", include_hidden: bool = True):
    """Diagnostics: every top-level window (even hidden or untitled) owned by a launched app's process tree,
    with title, class name, size, visibility. Use when the app runs but its window cannot be found."""
    s = _sessions.get(name)
    if not s:
        return _dump({"ok": False, "error": f"No session '{name}'. Running: {list(_sessions)}"})
    wins = s.owned_windows(include_hidden=include_hidden)
    return _dump(
        {
            "ok": True,
            "running": s.is_running(),
            "exit_code": s.exit_code(),
            "tracked_pids": sorted(s.pids()),
            "windows": [w.to_dict() for w in wins],
        }
    )


def _cleanup():
    for s in list(_sessions.values()):
        try:
            s.stop(force=True)
        except Exception:
            pass


atexit.register(_cleanup)

# --------------------------------------------------------------------------------------
# Windows
# --------------------------------------------------------------------------------------


@mcp.tool()
def list_windows(filter: Optional[str] = None):
    """List visible top-level windows (optionally filtered by title substring)."""
    wins = WindowManager.list_windows(title_filter=filter, only_visible=True)
    return _dump({"count": len(wins), "windows": [w.to_dict() for w in wins]})


@mcp.tool()
def set_default_window(window: str):
    """Set the default window (title substring) used when a tool is called without `window`."""
    global _default_window
    w = WindowManager.find_window(window, pids=_session_pids() or None)
    if not w:
        return _dump({"ok": False, "error": f"Window '{window}' not found"})
    _default_window = window
    return _dump({"ok": True, "window": w.to_dict()})


@mcp.tool()
def focus_window(window: Optional[str] = None):
    """Bring a window to the foreground (restores it if minimized)."""
    w = _prep(window)
    return _dump({"ok": True, "window": w.to_dict()})


@mcp.tool()
def set_window_geometry(width: int, height: int, x: int = 100, y: int = 100, window: Optional[str] = None):
    """Move/resize the window to a fixed size - gives reproducible screenshots and template matches."""
    w = _win(window)
    WindowManager.focus(w.hwnd)
    WindowManager.set_geometry(w.hwnd, x, y, width, height)
    time.sleep(0.3)
    w2 = WindowManager.get_window_info(w.hwnd) or w
    return _dump({"ok": True, "window": w2.to_dict()})


# --------------------------------------------------------------------------------------
# Seeing
# --------------------------------------------------------------------------------------


@mcp.tool()
def screenshot(window: Optional[str] = None, grid: int = 0, max_width: int = 1600, save_as: Optional[str] = None, focus: bool = True):
    """Take a screenshot of the app window and return it as an image.

    grid: if >0, overlays a coordinate grid every `grid` px labelled with window-relative x,y
          (e.g. grid=100). Very helpful for choosing click coordinates.
    max_width: downscale wide screenshots to save tokens (coordinates stay in original pixels).
    save_as: also save into screenshots/<save_as>.png (useful as manual baseline / for the user).
    window: title substring; omit for the default window. Pass window='*screen*' for the full desktop.
    """
    if window == "*screen*":
        img = ScreenCapture.capture_screen().convert("RGB")
        w = None
    else:
        w = _prep(window, focus=focus)
        img = _grab(w)

    if save_as:
        path = SHOTS_DIR / f"{_safe_name(save_as)}.png"
    else:
        path = SHOTS_DIR / f"auto_{datetime.now().strftime('%H%M%S_%f')}.png"
        _prune_shots()
    img.save(path)

    mimg, scale = _render(img, grid, max_width)
    info = {"ok": True, "window_size": [img.width, img.height], "saved": str(path)}
    if w:
        info["window_title"] = w.title
    if scale != 1.0:
        info["image_scale"] = round(scale, 4)
        info["note"] = "image is downscaled; tool coordinates are still ORIGINAL window pixels"
    return [_dump(info), mimg]


@mcp.tool()
def zoom(x: int, y: int, w: int, h: int, window: Optional[str] = None, factor: int = 3):
    """Crop a region of the window and magnify it - to inspect small icons, borders, text rendering."""
    win = _prep(window)
    img = _grab(win)
    x0, y0 = max(0, x), max(0, y)
    crop = img.crop((x0, y0, min(img.width, x0 + w), min(img.height, y0 + h)))
    crop = crop.resize((crop.width * factor, crop.height * factor), Image.NEAREST)
    mimg, _ = _render(crop, 0, 0)
    return [_dump({"ok": True, "region": [x0, y0, w, h], "factor": factor}), mimg]


@mcp.tool()
def pixel_color(x: int, y: int, window: Optional[str] = None):
    """RGB/hex color of one pixel (window-relative). Good for theme / hover-state assertions."""
    win = _prep(window)
    img = _grab(win)
    _check_xy(win, x, y)
    r, g, b = img.getpixel((x, y))
    return _dump({"ok": True, "x": x, "y": y, "rgb": [r, g, b], "hex": f"#{r:02x}{g:02x}{b:02x}"})


@mcp.tool()
def copy_screenshot_to_clipboard(window: Optional[str] = None):
    """Put a window screenshot on the Windows clipboard (CF_DIB) so the user can Ctrl+V it anywhere."""
    win = _prep(window)
    return _dump({"ok": ScreenCapture.copy_image_to_clipboard(_grab(win))})


# --------------------------------------------------------------------------------------
# Acting
# --------------------------------------------------------------------------------------


@mcp.tool()
def click(
    x: int,
    y: int,
    window: Optional[str] = None,
    button: str = "left",
    clicks: int = 1,
    screenshot_after: bool = False,
    delay: float = 0.3,
):
    """Click at window-relative (x, y). button: left|right|middle. clicks=2 for double-click."""
    w = _prep(window)
    ax, ay = _abs(w, x, y)
    InputController.click(ax, ay, button=button, clicks=clicks)
    return _done({"action": "click", "x": x, "y": y, "button": button, "clicks": clicks}, w, screenshot_after, delay)


@mcp.tool()
def click_template(
    template: str,
    window: Optional[str] = None,
    threshold: float = 0.8,
    timeout: float = 5.0,
    offset_x: int = 0,
    offset_y: int = 0,
    button: str = "left",
    clicks: int = 1,
    multi_scale: bool = True,
    screenshot_after: bool = False,
    delay: float = 0.3,
):
    """Find a template image (templates/<name>.png) in the window and click its center (+offset).
    multi_scale tolerates DPI / UI-scale differences. Fails with the best score if not found."""
    w = _prep(window)
    m, best = _locate(w, template, threshold, multi_scale, timeout)
    if not m:
        raise ValueError(f"Template '{template}' not found (best score {best['score']:.3f} < {threshold})" if best else f"Template '{template}' not found")
    cx, cy = m["cx"] + offset_x, m["cy"] + offset_y
    ax, ay = _abs(w, cx, cy)
    InputController.click(ax, ay, button=button, clicks=clicks)
    return _done({"action": "click_template", "template": template, "match": m, "clicked": [cx, cy]}, w, screenshot_after, delay)


@mcp.tool()
def click_text(
    text: str,
    window: Optional[str] = None,
    exact: bool = False,
    case_sensitive: bool = False,
    index: int = 0,
    button: str = "left",
    clicks: int = 1,
    screenshot_after: bool = False,
    delay: float = 0.3,
):
    """Find text on screen via OCR (needs: pip install rapidocr-onnxruntime) and click it.
    index selects among several matches (sorted top-to-bottom, left-to-right)."""
    w = _prep(window)
    items = _find_text_items(_grab(w), text, exact, case_sensitive)
    if len(items) <= index:
        raise ValueError(f"Text '{text}' not found via OCR ({len(items)} matches). Try read_text to see what OCR sees.")
    it = items[index]
    ax, ay = _abs(w, it["cx"], it["cy"])
    InputController.click(ax, ay, button=button, clicks=clicks)
    return _done({"action": "click_text", "match": it, "matches": len(items)}, w, screenshot_after, delay)


@mcp.tool()
def hover(x: int, y: int, window: Optional[str] = None, duration: float = 0.2, screenshot_after: bool = False, delay: float = 0.5):
    """Move the mouse to (x, y) to trigger hover styles / tooltips (delay = wait before screenshot)."""
    w = _prep(window)
    ax, ay = _abs(w, x, y)
    InputController.hover(ax, ay, duration=duration)
    return _done({"action": "hover", "x": x, "y": y}, w, screenshot_after, delay)


@mcp.tool()
def drag(x1: int, y1: int, x2: int, y2: int, window: Optional[str] = None, duration: float = 0.5, screenshot_after: bool = False):
    """Drag with the left button from (x1,y1) to (x2,y2) - resizing panels, reordering tabs, selecting."""
    w = _prep(window)
    a = _abs(w, x1, y1)
    b = _abs(w, x2, y2)
    InputController.drag(a[0], a[1], b[0], b[1], duration=duration)
    return _done({"action": "drag", "from": [x1, y1], "to": [x2, y2]}, w, screenshot_after, 0.3)


@mcp.tool()
def scroll(x: int, y: int, amount: int, window: Optional[str] = None, screenshot_after: bool = False):
    """Mouse wheel at (x, y). amount > 0 scrolls up, < 0 scrolls down (in wheel clicks)."""
    w = _prep(window)
    ax, ay = _abs(w, x, y)
    InputController.scroll(ax, ay, amount)
    return _done({"action": "scroll", "x": x, "y": y, "amount": amount}, w, screenshot_after, 0.3)


@mcp.tool()
def type_text(text: str, window: Optional[str] = None, method: str = "auto", screenshot_after: bool = False):
    """Type text into the focused control. method: auto|keys|paste.
    'auto' uses real key presses for ASCII and clipboard-paste for anything else (Cyrillic etc.).
    Paste overwrites the text clipboard."""
    w = _prep(window)
    use_paste = method == "paste" or (method == "auto" and not text.isascii())
    if use_paste:
        InputController.type_unicode(text)
    else:
        InputController.type_text(text)
    return _done({"action": "type_text", "chars": len(text), "method": "paste" if use_paste else "keys"}, w, screenshot_after, 0.2)


@mcp.tool()
def press_key(key: str, presses: int = 1, window: Optional[str] = None, screenshot_after: bool = False):
    """Press a single key (enter, esc, tab, up, down, f2, backspace, ...) one or more times."""
    w = _prep(window)
    pg = _pg()
    for _ in range(max(1, presses)):
        pg.press(key)
    return _done({"action": "press_key", "key": key, "presses": presses}, w, screenshot_after, 0.25)


@mcp.tool()
def hotkey(keys: list[str], window: Optional[str] = None, screenshot_after: bool = False):
    """Send a key combination, e.g. ["ctrl","p"], ["ctrl","shift","p"], ["alt","f4"]."""
    w = _prep(window)
    InputController.send_hotkey(*keys)
    return _done({"action": "hotkey", "keys": keys}, w, screenshot_after, 0.3)


@mcp.tool()
def wait(seconds: float):
    """Sleep. Prefer wait_for_change / wait_until_stable / wait_for_template."""
    time.sleep(max(0.0, min(seconds, 60.0)))
    return _dump({"ok": True, "waited": seconds})


@mcp.tool()
def clipboard_get_text():
    """Read the text currently on the clipboard (e.g. after the app's 'Copy' action)."""
    return _dump({"ok": True, "text": Clipboard.get_text()})


@mcp.tool()
def clipboard_set_text(text: str):
    """Put text on the clipboard."""
    return _dump({"ok": Clipboard.set_text(text)})


# --------------------------------------------------------------------------------------
# Finding & waiting
# --------------------------------------------------------------------------------------


@mcp.tool()
def find_template(template: str, window: Optional[str] = None, threshold: float = 0.8, multi_scale: bool = True, find_all: bool = False):
    """Locate a template image in the window; returns window-relative x,y,w,h,cx,cy and score.
    find_all=true returns every match (list of icons/rows)."""
    w = _prep(window, focus=False)
    img = _grab(w)
    if find_all:
        ms = ImageSearch.find_all(_resolve_template(template), _to_bgr(img), threshold=threshold)
        for m in ms:
            m["cx"], m["cy"] = m["center"]
            m.pop("center", None)
        return _dump({"ok": bool(ms), "count": len(ms), "matches": ms})
    m, best = _locate(w, template, threshold, multi_scale, 0.0)
    if m:
        return _dump({"ok": True, "found": True, **m})
    return _dump({"ok": False, "found": False, "best_score": round(best["score"], 4) if best else None, "threshold": threshold})


@mcp.tool()
def read_text(window: Optional[str] = None, region: Optional[list[int]] = None):
    """OCR the window (or region [x,y,w,h]); returns text boxes with window-relative coordinates.
    Needs: pip install rapidocr-onnxruntime. Lets the agent 'read' labels/menus/tabs of a GPU-rendered UI."""
    w = _prep(window, focus=False)
    img = _grab(w)
    ox = oy = 0
    if region:
        ox, oy, rw, rh = region
        img = img.crop((ox, oy, ox + rw, oy + rh))
    items = _ocr_items(img)
    for it in items:
        it["x"] += ox
        it["y"] += oy
        it["cx"] += ox
        it["cy"] += oy
    items.sort(key=lambda i: (i["y"], i["x"]))
    return _dump({"ok": True, "count": len(items), "items": items})


@mcp.tool()
def wait_for_template(template: str, window: Optional[str] = None, timeout: float = 10.0, threshold: float = 0.8, disappear: bool = False, multi_scale: bool = True):
    """Wait until a template appears (or disappears with disappear=true)."""
    w = _prep(window, focus=False)
    m, best = _locate(w, template, threshold, multi_scale, timeout, present=not disappear)
    if m or (disappear and not m and (best is None or best["score"] < threshold)):
        return _dump({"ok": True, "state": "disappeared" if disappear else "appeared", "match": m})
    raise TimeoutError(f"Template '{template}' did not {'disappear' if disappear else 'appear'} within {timeout}s (best score {best['score']:.3f})" if best else "timeout")


@mcp.tool()
def wait_for_text(text: str, window: Optional[str] = None, timeout: float = 10.0, exact: bool = False, disappear: bool = False):
    """Wait until OCR sees `text` in the window (or stops seeing it with disappear=true)."""
    w = _prep(window, focus=False)
    deadline = time.time() + timeout
    while True:
        found = bool(_find_text_items(_grab(w), text, exact, False))
        if found != disappear:
            return _dump({"ok": True, "state": "disappeared" if disappear else "appeared"})
        if time.time() >= deadline:
            raise TimeoutError(f"Text '{text}' did not {'disappear' if disappear else 'appear'} within {timeout}s")
        time.sleep(0.4)


@mcp.tool()
def wait_for_change(window: Optional[str] = None, timeout: float = 5.0, min_diff: float = 0.001, screenshot_after: bool = True):
    """Wait until the window content differs from NOW by more than min_diff (fraction of pixels).
    Call it right before the action that should change the UI, or right after - it compares to the
    first frame it sees. Raises on timeout (= the UI did not react)."""
    w = _prep(window, focus=False)
    base = _grab(w)
    deadline = time.time() + timeout
    while time.time() < deadline:
        time.sleep(0.12)
        cur = _grab(w)
        r = _ratio(base, cur)
        if r > min_diff:
            return _done({"changed": True, "diff_ratio": round(r, 5)}, w, screenshot_after, 0.25)
    raise TimeoutError(f"UI did not change within {timeout}s")


@mcp.tool()
def wait_until_stable(window: Optional[str] = None, timeout: float = 8.0, stable_for: float = 0.5, screenshot_after: bool = True):
    """Wait until the window stops changing (animations / loading finished) for `stable_for` seconds."""
    w = _prep(window, focus=False)
    prev = _grab(w)
    last_change = time.time()
    deadline = time.time() + timeout
    while time.time() < deadline:
        time.sleep(0.1)
        cur = _grab(w)
        if _ratio(prev, cur, 8) > 0.0:
            last_change = time.time()
        prev = cur
        if time.time() - last_change >= stable_for:
            return _done({"stable": True}, w, screenshot_after, 0.0)
    raise TimeoutError(f"UI kept changing for {timeout}s")


# --------------------------------------------------------------------------------------
# Test support: templates, assertions, baselines
# --------------------------------------------------------------------------------------


@mcp.tool()
def save_template(name: str, x: int, y: int, w: int, h: int, window: Optional[str] = None):
    """Cut a region out of the current window and save it as templates/<name>.png for find/click_template.
    Choose tight crops of distinctive icons/labels; avoid regions containing changing content."""
    win = _prep(window)
    img = _grab(win)
    crop = img.crop((x, y, x + w, y + h))
    path = TEMPLATES_DIR / f"{_safe_name(name)}.png"
    crop.save(path)
    mimg, _ = _render(crop.resize((crop.width * 3, crop.height * 3), Image.NEAREST), 0, 0)
    return [_dump({"ok": True, "saved": str(path), "size": [crop.width, crop.height]}), mimg]


@mcp.tool()
def assert_visible(
    template: Optional[str] = None,
    text: Optional[str] = None,
    window: Optional[str] = None,
    present: bool = True,
    threshold: float = 0.8,
    timeout: float = 3.0,
):
    """Assertion for tests: PASS if the template/text is visible (or absent with present=false)
    within `timeout`; otherwise FAIL plus a screenshot of the failing state."""
    if not template and not text:
        raise ValueError("Provide template or text")
    w = _prep(window, focus=False)
    deadline = time.time() + timeout
    ok = False
    detail = ""
    while True:
        if template:
            m, best = _locate(w, template, threshold, True, 0.0)
            seen = m is not None
            detail = f"template '{template}' best_score={best['score']:.3f}" if best else f"template '{template}'"
        else:
            seen = bool(_find_text_items(_grab(w), text, False, False))
            detail = f"text '{text}'"
        ok = seen == present
        if ok or time.time() >= deadline:
            break
        time.sleep(0.3)
    if ok:
        return _dump({"result": "PASS", "check": detail, "present": present})
    mimg, _ = _render(_grab(w), 0, 1600)
    return [_dump({"result": "FAIL", "check": detail, "expected_present": present}), mimg]


@mcp.tool()
def compare_to_baseline(name: str, window: Optional[str] = None, tolerance: float = 0.001, update: bool = False):
    """Visual regression: compare the window with baselines/<name>.png.
    First call (or update=true) stores the baseline. On mismatch returns the diff image with changed
    regions outlined in red. Use set_window_geometry first for stable sizes."""
    w = _prep(window)
    cur = _grab(w)
    path = BASELINES_DIR / f"{_safe_name(name)}.png"
    if update or not path.exists():
        cur.save(path)
        return _dump({"result": "BASELINE_SAVED", "path": str(path), "size": [cur.width, cur.height]})
    diff_path = SHOTS_DIR / f"diff_{_safe_name(name)}.png"
    res = VisualDiff.compare(path, cur, diff_output_path=diff_path, tolerance=tolerance)
    res["result"] = "PASS" if res.get("identical") else "FAIL"
    res["baseline"] = str(path)
    if res["result"] == "PASS" or not diff_path.exists():
        return _dump(res)
    mimg, _ = _render(Image.open(diff_path), 0, 1600)
    return [_dump(res), mimg]


@mcp.tool()
def diff_images(baseline: str, current: str, tolerance: float = 0.001):
    """Compare two image files (paths) and return the diff ratio + changed boxes; writes screenshots/diff_custom.png."""
    out = SHOTS_DIR / "diff_custom.png"
    res = VisualDiff.compare(baseline, current, diff_output_path=out, tolerance=tolerance)
    res["diff_image"] = str(out) if out.exists() else None
    return _dump(res)


# --------------------------------------------------------------------------------------
# Batch
# --------------------------------------------------------------------------------------

_BATCHABLE = {
    "click": click,
    "click_template": click_template,
    "click_text": click_text,
    "hover": hover,
    "drag": drag,
    "scroll": scroll,
    "type_text": type_text,
    "press_key": press_key,
    "hotkey": hotkey,
    "wait": wait,
    "wait_for_change": wait_for_change,
    "wait_until_stable": wait_until_stable,
    "wait_for_template": wait_for_template,
    "wait_for_text": wait_for_text,
    "assert_visible": assert_visible,
    "focus_window": focus_window,
    "set_window_geometry": set_window_geometry,
}


@mcp.tool()
def run_steps(steps: list[dict], window: Optional[str] = None, final_screenshot: bool = True, stop_on_error: bool = True):
    """Run several actions in one call to save round trips.

    steps: list of {"action": "<tool name>", ...that tool's arguments...}, e.g.
      [{"action":"hotkey","keys":["ctrl","p"]},
       {"action":"wait_for_change"},
       {"action":"type_text","text":"readme"},
       {"action":"press_key","key":"enter"},
       {"action":"assert_visible","text":"README.md"}]
    Allowed actions: click, click_template, click_text, hover, drag, scroll, type_text, press_key,
    hotkey, wait, wait_for_change, wait_until_stable, wait_for_template, wait_for_text,
    assert_visible, focus_window, set_window_geometry.
    A step whose assert_visible returns FAIL also stops the batch. Returns a per-step log
    and a final screenshot."""
    log = []
    failed = False
    for i, step in enumerate(steps):
        step = dict(step)
        act = step.pop("action", None)
        fn = _BATCHABLE.get(act)
        if fn is None:
            log.append({"step": i, "action": act, "ok": False, "error": f"unknown action; allowed: {sorted(_BATCHABLE)}"})
            failed = True
        else:
            if window and "window" not in step and act not in ("wait",):
                step["window"] = window
            if "screenshot_after" in fn.__code__.co_varnames:
                step["screenshot_after"] = False
            try:
                out = _text_of(fn(**step))
                entry = {"step": i, "action": act, "result": out[:400]}
                if '"result": "FAIL"' in out:
                    entry["ok"] = False
                    failed = True
                else:
                    entry["ok"] = True
                log.append(entry)
            except Exception as e:  # noqa: BLE001
                log.append({"step": i, "action": act, "ok": False, "error": f"{type(e).__name__}: {e}"})
                failed = True
        if failed and stop_on_error:
            break

    summary = {"ok": not failed, "executed": len(log), "total": len(steps), "log": log}
    if final_screenshot:
        try:
            w = _prep(window, focus=False)
            mimg, _ = _render(_grab(w), 0, 1600)
            return [_dump(summary), mimg]
        except Exception as e:  # noqa: BLE001
            summary["screenshot_error"] = str(e)
    return _dump(summary)


# --------------------------------------------------------------------------------------


def main():
    import argparse

    ap = argparse.ArgumentParser(description="GUI Tester MCP server")
    ap.add_argument("--transport", default="stdio", choices=["stdio", "sse", "streamable-http"])
    args = ap.parse_args()
    mcp.run(transport=args.transport)


if __name__ == "__main__":
    main()
