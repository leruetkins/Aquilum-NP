"""
Core module for GUI / Visual automation on Windows.
Provides window management, multi-monitor screen capture, template matching (OpenCV),
mouse/keyboard simulation, Windows clipboard image transfer, and visual diffs.
"""

from __future__ import annotations

import io
import sys
import time
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, List, Tuple, Dict, Any

import cv2
import numpy as np
from PIL import Image, ImageGrab

# Windows ctypes definitions
import ctypes
from ctypes import wintypes

user32 = ctypes.windll.user32
kernel32 = ctypes.windll.kernel32
dwmapi = ctypes.windll.dwmapi
gdi32 = ctypes.windll.gdi32

# 2. Try to enable per-monitor DPI awareness
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)  # PROCESS_PER_MONITOR_DPI_AWARE
except Exception:
    try:
        user32.SetProcessDPIAware()
    except Exception:
        pass

# 3. Setup ctypes argtypes and restypes for 64-bit safety
kernel32.GlobalAlloc.argtypes = [wintypes.UINT, ctypes.c_size_t]
kernel32.GlobalAlloc.restype = wintypes.HGLOBAL
kernel32.GlobalLock.argtypes = [wintypes.HGLOBAL]
kernel32.GlobalLock.restype = ctypes.c_void_p
kernel32.GlobalUnlock.argtypes = [wintypes.HGLOBAL]
kernel32.GlobalUnlock.restype = wintypes.BOOL

user32.OpenClipboard.argtypes = [wintypes.HWND]
user32.OpenClipboard.restype = wintypes.BOOL
user32.EmptyClipboard.argtypes = []
user32.EmptyClipboard.restype = wintypes.BOOL
user32.SetClipboardData.argtypes = [wintypes.UINT, wintypes.HANDLE]
user32.SetClipboardData.restype = wintypes.HANDLE
user32.CloseClipboard.argtypes = []
user32.CloseClipboard.restype = wintypes.BOOL

WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.EnumWindows.argtypes = [WNDENUMPROC, wintypes.LPARAM]
user32.EnumWindows.restype = wintypes.BOOL

user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
user32.GetWindowTextLengthW.restype = ctypes.c_int
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetWindowTextW.restype = ctypes.c_int

user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowThreadProcessId.restype = wintypes.DWORD
user32.GetClipboardData.argtypes = [wintypes.UINT]
user32.GetClipboardData.restype = wintypes.HANDLE

user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.IsWindowVisible.restype = wintypes.BOOL
user32.IsIconic.argtypes = [wintypes.HWND]
user32.IsIconic.restype = wintypes.BOOL
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
user32.ShowWindow.restype = wintypes.BOOL
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.SetForegroundWindow.restype = wintypes.BOOL
user32.BringWindowToTop.argtypes = [wintypes.HWND]
user32.BringWindowToTop.restype = wintypes.BOOL
user32.MoveWindow.argtypes = [wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.BOOL]
user32.MoveWindow.restype = wintypes.BOOL


@dataclass
class WindowInfo:
    hwnd: int
    title: str
    x: int
    y: int
    width: int
    height: int
    is_visible: bool
    is_minimized: bool
    pid: int = 0
    class_name: str = ""

    @property
    def rect(self) -> Tuple[int, int, int, int]:
        """(left, top, right, bottom)"""
        return (self.x, self.y, self.x + self.width, self.y + self.height)

    @property
    def center(self) -> Tuple[int, int]:
        return (self.x + self.width // 2, self.y + self.height // 2)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "hwnd": self.hwnd,
            "title": self.title,
            "x": self.x,
            "y": self.y,
            "width": self.width,
            "height": self.height,
            "center": list(self.center),
            "is_visible": self.is_visible,
            "is_minimized": self.is_minimized,
            "pid": self.pid,
            "class_name": self.class_name,
        }


class WindowManager:
    """Finds, focuses, moves, and inspects Windows desktop windows."""

    @staticmethod
    def _get_accurate_rect(hwnd: int) -> Tuple[int, int, int, int]:
        """
        Uses DwmGetWindowAttribute (DWMWA_EXTENDED_FRAME_BOUNDS) to get the true
        visible window frame without transparent drop shadows.
        """
        try:
            rect = wintypes.RECT()
            DWMWA_EXTENDED_FRAME_BOUNDS = 9
            res = dwmapi.DwmGetWindowAttribute(
                wintypes.HWND(hwnd),
                wintypes.DWORD(DWMWA_EXTENDED_FRAME_BOUNDS),
                ctypes.byref(rect),
                ctypes.sizeof(rect),
            )
            if res == 0:
                return (rect.left, rect.top, rect.right, rect.bottom)
        except Exception:
            pass

        # Fallback to GetWindowRect
        rect = wintypes.RECT()
        user32.GetWindowRect(hwnd, ctypes.byref(rect))
        return (rect.left, rect.top, rect.right, rect.bottom)

    @classmethod
    def get_window_info(cls, hwnd: int) -> Optional[WindowInfo]:
        if not user32.IsWindow(hwnd):
            return None

        title_len = user32.GetWindowTextLengthW(hwnd)
        if title_len > 0:
            buf = ctypes.create_unicode_buffer(title_len + 1)
            user32.GetWindowTextW(hwnd, buf, title_len + 1)
            title = buf.value
        else:
            title = ""

        is_visible = bool(user32.IsWindowVisible(hwnd))
        is_minimized = bool(user32.IsIconic(hwnd))

        pid_val = wintypes.DWORD(0)
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid_val))
        cls_buf = ctypes.create_unicode_buffer(256)
        user32.GetClassNameW(hwnd, cls_buf, 256)

        left, top, right, bottom = cls._get_accurate_rect(hwnd)
        width = max(0, right - left)
        height = max(0, bottom - top)

        return WindowInfo(
            hwnd=hwnd,
            title=title,
            x=left,
            y=top,
            width=width,
            height=height,
            is_visible=is_visible,
            is_minimized=is_minimized,
            pid=int(pid_val.value),
            class_name=cls_buf.value,
        )

    @classmethod
    def list_windows(cls, title_filter: Optional[str] = None, only_visible: bool = True) -> List[WindowInfo]:
        """Returns all matching top-level windows."""
        results: List[WindowInfo] = []

        def enum_callback(hwnd, _lparam):
            if only_visible and not user32.IsWindowVisible(hwnd):
                return 1

            title_len = user32.GetWindowTextLengthW(hwnd)
            if title_len <= 0:
                return 1

            buf = ctypes.create_unicode_buffer(title_len + 1)
            user32.GetWindowTextW(hwnd, buf, title_len + 1)
            title = buf.value.strip()

            if not title:
                return 1

            if title_filter and title_filter.lower() not in title.lower():
                return 1

            info = cls.get_window_info(hwnd)
            if info and info.width > 0 and info.height > 0:
                results.append(info)
            return 1

        proc = WNDENUMPROC(enum_callback)
        user32.EnumWindows(proc, 0)
        if not results:
            try:
                hdesk = user32.OpenInputDesktop(0, False, 0x01FF)
                if hdesk:
                    cur_desk = user32.GetThreadDesktop(kernel32.GetCurrentThreadId())
                    user32.SetThreadDesktop(hdesk)
                    user32.EnumWindows(proc, 0)
                    user32.SetThreadDesktop(cur_desk)
            except Exception:
                pass
        return results

    @classmethod
    def windows_of_pids(cls, pids, include_hidden: bool = False, include_untitled: bool = True) -> List[WindowInfo]:
        """All top-level windows owned by the given process ids (any title, optionally hidden ones)."""
        results: List[WindowInfo] = []
        pids = set(pids or ())

        def enum_callback(hwnd, _lparam):
            owner = wintypes.DWORD(0)
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value not in pids:
                return 1
            if not include_hidden and not user32.IsWindowVisible(hwnd):
                return 1
            info = cls.get_window_info(hwnd)
            if info and (include_untitled or info.title):
                results.append(info)
            return 1

        proc = WNDENUMPROC(enum_callback)
        user32.EnumWindows(proc, 0)
        if not results:
            try:
                hdesk = user32.OpenInputDesktop(0, False, 0x01FF)
                if hdesk:
                    cur_desk = user32.GetThreadDesktop(kernel32.GetCurrentThreadId())
                    user32.SetThreadDesktop(hdesk)
                    user32.EnumWindows(proc, 0)
                    user32.SetThreadDesktop(cur_desk)
            except Exception:
                pass
        return results

    @classmethod
    def find_window(
        cls,
        title_substring: str,
        exact: bool = False,
        pids: Optional[set] = None,
    ) -> Optional[WindowInfo]:
        """
        Finds a visible window whose title contains title_substring.
        - exact: title must equal title_substring (case-insensitive)
        - pids: prefer windows owned by these process ids (e.g. the app we launched), so a
          terminal/IDE whose title merely mentions the app name is not picked by mistake.
        """
        matches = cls.list_windows(title_filter=title_substring, only_visible=True)
        if exact:
            matches = [m for m in matches if m.title.strip().lower() == title_substring.strip().lower()]
        if pids:
            owned = [m for m in matches if m.pid in pids]
            if owned:
                matches = owned
        return matches[0] if matches else None

    @classmethod
    def focus(cls, hwnd: int) -> bool:
        """Brings the window to front and activates it."""
        try:
            SW_RESTORE = 9
            SW_SHOW = 5
            if user32.IsIconic(hwnd):
                user32.ShowWindow(hwnd, SW_RESTORE)
            else:
                user32.ShowWindow(hwnd, SW_SHOW)

            current_thread = kernel32.GetCurrentThreadId()
            target_thread = user32.GetWindowThreadProcessId(hwnd, None)
            user32.AttachThreadInput(target_thread, current_thread, True)
            user32.SetForegroundWindow(hwnd)
            user32.BringWindowToTop(hwnd)
            user32.AttachThreadInput(target_thread, current_thread, False)
            time.sleep(0.15)
            return True
        except Exception as e:
            print(f"[WindowManager.focus] Warning: {e}", file=sys.stderr)
            return False

    @classmethod
    def set_geometry(cls, hwnd: int, x: int, y: int, width: int, height: int) -> bool:
        """Positions and resizes window to fixed bounds."""
        try:
            user32.MoveWindow(hwnd, x, y, width, height, True)
            time.sleep(0.1)
            return True
        except Exception as e:
            print(f"[WindowManager.set_geometry] Warning: {e}", file=sys.stderr)
            return False


class ScreenCapture:
    """Captures desktop, specific windows, or subregions, and writes directly to clipboard."""

    @staticmethod
    def capture_screen(output_path: Optional[str | Path] = None) -> Image.Image:
        """Takes full virtual desktop screenshot across all monitors."""
        img = ImageGrab.grab(all_screens=True)
        if output_path:
            p = Path(output_path)
            p.parent.mkdir(parents=True, exist_ok=True)
            img.save(p)
        return img

    @staticmethod
    def capture_rect(
        x: int, y: int, width: int, height: int, output_path: Optional[str | Path] = None
    ) -> Image.Image:
        """Captures a rectangle in screen coordinates."""
        bbox = (int(x), int(y), int(x + width), int(y + height))
        img = ImageGrab.grab(bbox=bbox, all_screens=True)

        if output_path:
            p = Path(output_path)
            p.parent.mkdir(parents=True, exist_ok=True)
            img.save(p)
        return img

    @classmethod
    def capture_window_hwnd(cls, hwnd: int) -> Optional[Image.Image]:
        """
        Directly captures a window via PrintWindow (PW_RENDERFULLCONTENT).
        Works across Windows desktops and even if the window is occluded or in the background.
        """
        try:
            r = wintypes.RECT()
            user32.GetClientRect(hwnd, ctypes.byref(r))
            w, h = r.right - r.left, r.bottom - r.top
            if w <= 0 or h <= 0:
                user32.GetWindowRect(hwnd, ctypes.byref(r))
                w, h = r.right - r.left, r.bottom - r.top
            if w <= 0 or h <= 0:
                return None

            hdc_win = user32.GetDC(hwnd)
            if not hdc_win:
                return None
            hdc_mem = gdi32.CreateCompatibleDC(hdc_win)
            hbmp = gdi32.CreateCompatibleBitmap(hdc_win, w, h)
            gdi32.SelectObject(hdc_mem, hbmp)

            PW_RENDERFULLCONTENT = 2
            ok = user32.PrintWindow(hwnd, hdc_mem, PW_RENDERFULLCONTENT)
            if not ok:
                ok = user32.PrintWindow(hwnd, hdc_mem, 0)

            img = None
            if ok:
                class BITMAPINFOHEADER(ctypes.Structure):
                    _fields_ = [
                        ("biSize", wintypes.DWORD),
                        ("biWidth", wintypes.LONG),
                        ("biHeight", wintypes.LONG),
                        ("biPlanes", wintypes.WORD),
                        ("biBitCount", wintypes.WORD),
                        ("biCompression", wintypes.DWORD),
                        ("biSizeImage", wintypes.DWORD),
                        ("biXPelsPerMeter", wintypes.LONG),
                        ("biYPelsPerMeter", wintypes.LONG),
                        ("biClrUsed", wintypes.DWORD),
                        ("biClrImportant", wintypes.DWORD),
                    ]

                bmi = BITMAPINFOHEADER()
                bmi.biSize = ctypes.sizeof(BITMAPINFOHEADER)
                bmi.biWidth = w
                bmi.biHeight = -h  # top-down
                bmi.biPlanes = 1
                bmi.biBitCount = 32
                bmi.biCompression = 0  # BI_RGB

                buf = ctypes.create_string_buffer(w * h * 4)
                gdi32.GetDIBits(hdc_mem, hbmp, 0, h, buf, ctypes.byref(bmi), 0)
                img = Image.frombuffer("RGBA", (w, h), buf, "raw", "BGRA", 0, 1).convert("RGB")

            user32.ReleaseDC(hwnd, hdc_win)
            gdi32.DeleteDC(hdc_mem)
            gdi32.DeleteObject(hbmp)
            return img
        except Exception as e:
            print(f"[ScreenCapture.capture_window_hwnd] Warning: {e}", file=sys.stderr)
            return None

    @classmethod
    def capture_window(
        cls,
        target: int | str | WindowInfo,
        output_path: Optional[str | Path] = None,
        bring_to_front: bool = True,
    ) -> Optional[Image.Image]:
        """Captures the target window by HWND, title substring, or WindowInfo object."""
        if isinstance(target, WindowInfo):
            info = target
        elif isinstance(target, int):
            info = WindowManager.get_window_info(target)
        else:
            info = WindowManager.find_window(target)

        if not info:
            print(f"[ScreenCapture.capture_window] Window not found: {target}", file=sys.stderr)
            return None

        if bring_to_front:
            WindowManager.focus(info.hwnd)
            time.sleep(0.15)
            # Re-read rect after focusing/restoring
            info = WindowManager.get_window_info(info.hwnd) or info

        # 1. Try accurate PrintWindow capture directly from HWND
        img = cls.capture_window_hwnd(info.hwnd)

        # 2. Fall back to screen capture if PrintWindow returned None
        if img is None:
            try:
                img = cls.capture_rect(info.x, info.y, info.width, info.height)
            except Exception:
                pass

        if img and output_path:
            p = Path(output_path)
            p.parent.mkdir(parents=True, exist_ok=True)
            img.save(p)
        return img

    @staticmethod
    def copy_image_to_clipboard(image_input: Image.Image | str | Path) -> bool:
        """
        Copies a PIL Image or image file path directly into the Windows clipboard as CF_DIB.
        Allows instant paste (Ctrl+V) into graphics editors, text editors, chat, etc.
        """
        if isinstance(image_input, (str, Path)):
            pil_img = Image.open(image_input)
        else:
            pil_img = image_input

        # Convert to BMP in memory, strip 14-byte BITMAPFILEHEADER to get CF_DIB
        output = io.BytesIO()
        pil_img.convert("RGB").save(output, "BMP")
        dib_data = output.getvalue()[14:]
        output.close()

        GMEM_MOVEABLE = 0x0002
        CF_DIB = 8

        try:
            h_mem = kernel32.GlobalAlloc(GMEM_MOVEABLE, len(dib_data))
            if not h_mem:
                return False
            p_mem = kernel32.GlobalLock(h_mem)
            if not p_mem:
                return False
            ctypes.memmove(p_mem, dib_data, len(dib_data))
            kernel32.GlobalUnlock(h_mem)

            if user32.OpenClipboard(None):
                user32.EmptyClipboard()
                res = user32.SetClipboardData(CF_DIB, h_mem)
                user32.CloseClipboard()
                return bool(res)
            return False
        except Exception as e:
            print(f"[ScreenCapture.copy_image_to_clipboard] Error: {e}", file=sys.stderr)
            return False

    @staticmethod
    def get_clipboard_image() -> Optional[Image.Image]:
        """Retrieves image from clipboard if available."""
        try:
            return ImageGrab.grabclipboard()
        except Exception:
            return None


class ImageSearch:
    """Fast template matching using OpenCV to locate buttons, icons, and menus on screen."""

    @staticmethod
    def _to_cv2(image_input: Image.Image | str | Path | np.ndarray) -> np.ndarray:
        if isinstance(image_input, np.ndarray):
            return image_input
        if isinstance(image_input, (str, Path)):
            img = cv2.imread(str(image_input))
            if img is None:
                raise FileNotFoundError(f"Cannot load image: {image_input}")
            return img
        # PIL Image
        return cv2.cvtColor(np.array(image_input), cv2.COLOR_RGB2BGR)

    @classmethod
    def find_template(
        cls,
        template_input: Image.Image | str | Path,
        source_input: Optional[Image.Image | str | Path | np.ndarray] = None,
        threshold: float = 0.8,
    ) -> Dict[str, Any]:
        """
        Locates the template image inside source_input (or current screen if source_input is None).
        Returns dict with: found, score, x, y, width, height, center.
        """
        if source_input is None:
            source_img = ScreenCapture.capture_screen()
        else:
            source_img = source_input

        source_cv = cls._to_cv2(source_img)
        template_cv = cls._to_cv2(template_input)

        s_h, s_w = source_cv.shape[:2]
        t_h, t_w = template_cv.shape[:2]

        if t_h > s_h or t_w > s_w:
            return {
                "found": False,
                "score": 0.0,
                "message": f"Template size ({t_w}x{t_h}) exceeds source size ({s_w}x{s_h})",
            }

        res = cv2.matchTemplate(source_cv, template_cv, cv2.TM_CCOEFF_NORMED)
        min_val, max_val, min_loc, max_loc = cv2.minMaxLoc(res)

        if max_val >= threshold:
            best_x, best_y = max_loc
            center_x = best_x + t_w // 2
            center_y = best_y + t_h // 2
            return {
                "found": True,
                "score": float(round(max_val, 4)),
                "x": int(best_x),
                "y": int(best_y),
                "width": int(t_w),
                "height": int(t_h),
                "center": (int(center_x), int(center_y)),
            }
        else:
            return {
                "found": False,
                "score": float(round(max_val, 4)),
                "threshold": threshold,
                "best_location": {"x": int(max_loc[0]), "y": int(max_loc[1])},
            }

    @classmethod
    def find_all(
        cls,
        template_input: Image.Image | str | Path,
        source_input: Optional[Image.Image | str | Path | np.ndarray] = None,
        threshold: float = 0.8,
        max_results: int = 20,
    ) -> List[Dict[str, Any]]:
        """Finds all occurrences of template matching the threshold."""
        if source_input is None:
            source_img = ScreenCapture.capture_screen()
        else:
            source_img = source_input

        source_cv = cls._to_cv2(source_img)
        template_cv = cls._to_cv2(template_input)
        t_h, t_w = template_cv.shape[:2]

        res = cv2.matchTemplate(source_cv, template_cv, cv2.TM_CCOEFF_NORMED)
        locs = np.where(res >= threshold)

        results: List[Dict[str, Any]] = []
        for pt in zip(*locs[::-1]):
            x, y = int(pt[0]), int(pt[1])
            too_close = False
            for r in results:
                if abs(r["x"] - x) < t_w // 2 and abs(r["y"] - y) < t_h // 2:
                    too_close = True
                    break
            if not too_close:
                score = float(round(res[y, x], 4))
                results.append(
                    {
                        "score": score,
                        "x": x,
                        "y": y,
                        "width": t_w,
                        "height": t_h,
                        "center": (x + t_w // 2, y + t_h // 2),
                    }
                )
                if len(results) >= max_results:
                    break

        return results

    @classmethod
    def wait_for_template(
        cls,
        template_input: Image.Image | str | Path,
        timeout: float = 10.0,
        interval: float = 0.5,
        threshold: float = 0.8,
        window_title: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Polls screen or window until template appears or timeout is reached."""
        start = time.time()
        while time.time() - start < timeout:
            if window_title:
                win = WindowManager.find_window(window_title)
                if win:
                    source = ScreenCapture.capture_window(win, bring_to_front=False)
                    match = cls.find_template(template_input, source, threshold)
                    if match["found"]:
                        match["x"] += win.x
                        match["y"] += win.y
                        match["center"] = (match["center"][0] + win.x, match["center"][1] + win.y)
                        return match
            else:
                match = cls.find_template(template_input, None, threshold)
                if match["found"]:
                    return match
            time.sleep(interval)

        return {"found": False, "score": 0.0, "message": f"Timeout waiting for template ({timeout}s)"}


class InputController:
    """Controls mouse and keyboard with precise coordinates and timing."""

    @staticmethod
    def click(x: int, y: int, button: str = "left", clicks: int = 1, interval: float = 0.1):
        """Moves to (x, y) and clicks."""
        import pyautogui
        pyautogui.moveTo(x, y)
        time.sleep(0.05)
        pyautogui.click(x=x, y=y, button=button, clicks=clicks, interval=interval)

    @staticmethod
    def hover(x: int, y: int, duration: float = 0.2):
        """Moves to (x, y) and waits to trigger tooltips or hover states."""
        import pyautogui
        pyautogui.moveTo(x, y, duration=duration)
        time.sleep(0.2)

    @staticmethod
    def drag(x1: int, y1: int, x2: int, y2: int, duration: float = 0.5):
        """Drags mouse from (x1, y1) to (x2, y2)."""
        import pyautogui
        pyautogui.moveTo(x1, y1)
        pyautogui.dragTo(x2, y2, duration=duration, button="left")

    @staticmethod
    def type_text(text: str, interval: float = 0.02):
        """Types string character by character."""
        import pyautogui
        pyautogui.write(text, interval=interval)

    @staticmethod
    def scroll(x: int, y: int, amount: int):
        """Scrolls the wheel at (x, y). Positive = up, negative = down."""
        import pyautogui
        pyautogui.moveTo(x, y)
        time.sleep(0.05)
        pyautogui.scroll(amount, x=x, y=y)

    @staticmethod
    def type_unicode(text: str):
        """
        Types arbitrary Unicode (Cyrillic, emoji...) by placing it on the clipboard and pasting.
        pyautogui.write() only handles ASCII. Overwrites the text clipboard.
        """
        import pyautogui
        if not Clipboard.set_text(text):
            raise RuntimeError("Could not set clipboard text")
        time.sleep(0.05)
        pyautogui.hotkey("ctrl", "v")
        time.sleep(0.1)

    @staticmethod
    def send_hotkey(*keys: str):
        """Sends key combinations (e.g. 'ctrl', 'v' or 'alt', 'f')."""
        import pyautogui
        pyautogui.hotkey(*keys)

    @staticmethod
    def press_key(key: str):
        """Presses single key (e.g. 'enter', 'esc', 'tab')."""
        import pyautogui
        pyautogui.press(key)

    @classmethod
    def click_template(
        cls,
        template_input: Image.Image | str | Path,
        threshold: float = 0.8,
        timeout: float = 5.0,
        button: str = "left",
        clicks: int = 1,
        offset: Tuple[int, int] = (0, 0),
        window_title: Optional[str] = None,
    ) -> bool:
        """Finds template and clicks its center (+ optional offset)."""
        match = ImageSearch.wait_for_template(
            template_input, timeout=timeout, threshold=threshold, window_title=window_title
        )
        if match["found"]:
            cx, cy = match["center"]
            target_x = cx + offset[0]
            target_y = cy + offset[1]
            cls.click(target_x, target_y, button=button, clicks=clicks)
            return True
        return False

    @classmethod
    def hover_template(
        cls,
        template_input: Image.Image | str | Path,
        threshold: float = 0.8,
        timeout: float = 5.0,
        offset: Tuple[int, int] = (0, 0),
        window_title: Optional[str] = None,
    ) -> bool:
        """Finds template and hovers over its center."""
        match = ImageSearch.wait_for_template(
            template_input, timeout=timeout, threshold=threshold, window_title=window_title
        )
        if match["found"]:
            cx, cy = match["center"]
            cls.hover(cx + offset[0], cy + offset[1])
            return True
        return False


class VisualDiff:
    """Compares two screenshots, calculates diff ratio, and generates visual diff image."""

    @staticmethod
    def compare(
        img_a_input: Image.Image | str | Path,
        img_b_input: Image.Image | str | Path,
        diff_output_path: Optional[str | Path] = None,
        tolerance: float = 0.01,
    ) -> Dict[str, Any]:
        """
        Compares baseline and current images.
        Returns match boolean, difference ratio (0.0 to 1.0), and saves diff highlighting if changed.
        """
        def load_cv(img_in):
            if isinstance(img_in, (str, Path)):
                return cv2.imread(str(img_in))
            if isinstance(img_in, Image.Image):
                return cv2.cvtColor(np.array(img_in), cv2.COLOR_RGB2BGR)
            return img_in

        cv_a = load_cv(img_a_input)
        cv_b = load_cv(img_b_input)

        if cv_a is None or cv_b is None:
            return {"identical": False, "error": "Could not load one or both images"}

        if cv_a.shape != cv_b.shape:
            cv_b = cv2.resize(cv_b, (cv_a.shape[1], cv_a.shape[0]))

        diff = cv2.absdiff(cv_a, cv_b)
        gray_diff = cv2.cvtColor(diff, cv2.COLOR_BGR2GRAY)
        _, thresh = cv2.threshold(gray_diff, 25, 255, cv2.THRESH_BINARY)

        total_pixels = thresh.shape[0] * thresh.shape[1]
        changed_pixels = cv2.countNonZero(thresh)
        diff_ratio = changed_pixels / total_pixels

        identical = diff_ratio <= tolerance

        bounding_boxes = []
        if not identical:
            contours, _ = cv2.findContours(thresh, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
            for cnt in contours:
                x, y, w, h = cv2.boundingRect(cnt)
                if w * h > 20:
                    bounding_boxes.append({"x": int(x), "y": int(y), "w": int(w), "h": int(h)})

            if diff_output_path:
                highlight = cv_b.copy()
                for b in bounding_boxes:
                    cv2.rectangle(
                        highlight,
                        (b["x"], b["y"]),
                        (b["x"] + b["w"], b["y"] + b["h"]),
                        (0, 0, 255),
                        2,
                    )
                p = Path(diff_output_path)
                p.parent.mkdir(parents=True, exist_ok=True)
                cv2.imwrite(str(p), highlight)

        return {
            "identical": bool(identical),
            "diff_ratio": float(round(diff_ratio, 6)),
            "changed_pixels": int(changed_pixels),
            "total_pixels": int(total_pixels),
            "bounding_boxes_count": len(bounding_boxes),
            "diff_boxes": bounding_boxes[:30],
        }


class Clipboard:
    """Text clipboard helpers (images: see ScreenCapture.copy_image_to_clipboard)."""

    CF_UNICODETEXT = 13

    @classmethod
    def set_text(cls, text: str) -> bool:
        data = (text + "\0").encode("utf-16-le")
        h_mem = kernel32.GlobalAlloc(0x0002, len(data))
        if not h_mem:
            return False
        p_mem = kernel32.GlobalLock(h_mem)
        if not p_mem:
            return False
        ctypes.memmove(p_mem, data, len(data))
        kernel32.GlobalUnlock(h_mem)
        if not user32.OpenClipboard(None):
            return False
        try:
            user32.EmptyClipboard()
            return bool(user32.SetClipboardData(cls.CF_UNICODETEXT, h_mem))
        finally:
            user32.CloseClipboard()

    @classmethod
    def get_text(cls) -> Optional[str]:
        if not user32.OpenClipboard(None):
            return None
        try:
            h = user32.GetClipboardData(cls.CF_UNICODETEXT)
            if not h:
                return None
            p = kernel32.GlobalLock(h)
            if not p:
                return None
            try:
                return ctypes.wstring_at(p)
            finally:
                kernel32.GlobalUnlock(h)
        finally:
            user32.CloseClipboard()


class AppSession:
    """
    Manages application lifecycle.
    - stdout/stderr of the app go to `log_path` (never to our own stdout: that would corrupt
      the MCP stdio protocol and also lets the agent read Rust panics afterwards).
    - stop() kills the whole process tree (important for `cargo run`, where the real app is a child).
    """

    def __init__(self, executable_or_cmd: str, working_dir: Optional[str] = None, log_path: Optional[str] = None):
        self.cmd = executable_or_cmd
        self.cwd = working_dir
        self.log_path = log_path
        self.process: Optional[subprocess.Popen] = None
        self.window_info: Optional[WindowInfo] = None
        self._log_fh = None

    def pids(self) -> set:
        """PIDs of the launched process and (if psutil is available) all its descendants."""
        result: set = set()
        if not self.process:
            return result
        result.add(self.process.pid)
        try:
            import psutil  # optional
            for child in psutil.Process(self.process.pid).children(recursive=True):
                result.add(child.pid)
        except Exception:
            pass
        return result

    def start(
        self,
        wait_secs: float = 2.0,
        window_title: Optional[str] = None,
        timeout: Optional[float] = None,
    ) -> bool:
        """Launches the process and optionally waits for its window to appear."""
        shell = isinstance(self.cmd, str) and (" " in self.cmd or "|" in self.cmd)

        if self.log_path:
            Path(self.log_path).parent.mkdir(parents=True, exist_ok=True)
            self._log_fh = open(self.log_path, "wb")
            out = self._log_fh
        else:
            out = subprocess.DEVNULL

        flags = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0) | getattr(subprocess, "CREATE_NO_WINDOW", 0)
        self.process = subprocess.Popen(
            self.cmd,
            cwd=self.cwd,
            shell=shell,
            stdin=subprocess.DEVNULL,
            stdout=out,
            stderr=subprocess.STDOUT,
            creationflags=flags,
        )

        start_time = time.time()
        timeout = timeout if timeout is not None else max(wait_secs, 10.0)

        if window_title:
            while time.time() - start_time < timeout:
                win = self.find_window(window_title)
                if win:
                    self.window_info = win
                    WindowManager.focus(win.hwnd)
                    return True
                rc = self.process.poll()
                if rc not in (None, 0):  # build/launch failed - no point waiting
                    return False
                time.sleep(0.3)
            return False

        time.sleep(wait_secs)
        return self.is_running()

    _IGNORED_CLASSES = ("ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS", "PseudoConsoleWindow")

    def owned_windows(self, include_hidden: bool = False) -> List[WindowInfo]:
        """Windows owned by the launched process tree, whatever their title."""
        return WindowManager.windows_of_pids(self.pids(), include_hidden=include_hidden)

    def find_window(self, title: Optional[str] = None, allow_any_owned: bool = True) -> Optional[WindowInfo]:
        """
        1) a title match owned by our process tree,
        2) (allow_any_owned) the biggest visible window of our process tree, whatever its title -
           GPUI/Slint windows often have an unexpected or empty title,
        3) any title match on the desktop (needed when the process tree is unknown, e.g. no psutil).
        """
        pids = self.pids()
        matches = WindowManager.list_windows(title_filter=title, only_visible=True) if title else []
        owned = [m for m in matches if m.pid in pids]
        if owned:
            return owned[0]
        if allow_any_owned:
            cands = [
                w for w in self.owned_windows()
                if w.width >= 100 and w.height >= 100 and w.class_name not in self._IGNORED_CLASSES
            ]
            if cands:
                return max(cands, key=lambda w: w.width * w.height)
        return matches[0] if matches else None

    def is_running(self) -> bool:
        return self.process is not None and self.process.poll() is None

    def exit_code(self) -> Optional[int]:
        return self.process.poll() if self.process else None

    def stop(self, force: bool = False):
        """Terminates the process tree."""
        if not self.process:
            return
        pid = self.process.pid
        try:
            if not force:
                self.process.terminate()
                try:
                    self.process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    pass
            if sys.platform == "win32":
                # /T = whole tree (cargo run -> app), /F = force
                subprocess.run(
                    ["taskkill", "/PID", str(pid), "/T", "/F"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
            elif self.process.poll() is None:
                self.process.kill()
        except Exception as e:
            print(f"[AppSession.stop] Warning: {e}", file=sys.stderr)
        finally:
            self.process = None
            if self._log_fh:
                try:
                    self._log_fh.close()
                except Exception:
                    pass
                self._log_fh = None
