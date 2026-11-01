"""
Command Line Interface for GUI Tester.
Designed for both human developers and autonomous AI agents.
Supports structured JSON output (--json) for seamless tool use by AI models.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Force utf-8 for Windows console output
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

# Add parent directory so we can run cli.py standalone
sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))

from tools.gui_tester.core import (
    WindowManager,
    ScreenCapture,
    ImageSearch,
    InputController,
    VisualDiff,
    AppSession,
)


def output_result(data: dict, as_json: bool = False):
    if as_json:
        print(json.dumps(data, indent=2, ensure_ascii=False))
    else:
        if data.get("success", True):
            print(f"[OK] {data.get('message', 'Operation successful')}")
            for k, v in data.items():
                if k not in ("success", "message"):
                    print(f"  {k}: {v}")
        else:
            print(f"[ERROR] {data.get('error', 'Operation failed')}", file=sys.stderr)
            for k, v in data.items():
                if k not in ("success", "error"):
                    print(f"  {k}: {v}", file=sys.stderr)


def cmd_list_windows(args):
    wins = WindowManager.list_windows(title_filter=args.filter, only_visible=not args.all)
    data = {
        "success": True,
        "count": len(wins),
        "windows": [w.to_dict() for w in wins],
    }
    output_result(data, args.json)


def cmd_focus_window(args):
    win = WindowManager.find_window(args.window)
    if not win:
        output_result({"success": False, "error": f"Window matching '{args.window}' not found"}, args.json)
        sys.exit(1)
    res = WindowManager.focus(win.hwnd)
    output_result({"success": res, "window": win.to_dict(), "message": f"Focused window '{win.title}'"}, args.json)


def cmd_capture(args):
    if args.window:
        win = WindowManager.find_window(args.window)
        if not win:
            output_result({"success": False, "error": f"Window '{args.window}' not found"}, args.json)
            sys.exit(1)
        img = ScreenCapture.capture_window(win, output_path=args.output, bring_to_front=not args.no_focus)
        rect_info = win.to_dict()
    elif args.rect:
        x, y, w, h = args.rect
        img = ScreenCapture.capture_rect(x, y, w, h, output_path=args.output)
        rect_info = {"x": x, "y": y, "width": w, "height": h}
    else:
        img = ScreenCapture.capture_screen(output_path=args.output)
        rect_info = {"screen": "fullscreen"}

    clipboard_status = False
    if args.to_clipboard and img:
        clipboard_status = ScreenCapture.copy_image_to_clipboard(img)

    output_result(
        {
            "success": img is not None,
            "output_path": str(args.output) if args.output else None,
            "copied_to_clipboard": clipboard_status,
            "size": list(img.size) if img else None,
            "target": rect_info,
            "message": f"Screenshot saved to {args.output}" if args.output else "Screenshot captured",
        },
        args.json,
    )


def cmd_find_image(args):
    source = None
    offset_x, offset_y = 0, 0

    if args.window:
        win = WindowManager.find_window(args.window)
        if not win:
            output_result({"success": False, "error": f"Window '{args.window}' not found"}, args.json)
            sys.exit(1)
        source = ScreenCapture.capture_window(win, bring_to_front=False)
        offset_x, offset_y = win.x, win.y

    if args.source:
        from PIL import Image
        source = Image.open(args.source)

    if args.all:
        matches = ImageSearch.find_all(args.template, source, threshold=args.threshold)
        for m in matches:
            m["x"] += offset_x
            m["y"] += offset_y
            m["center"] = (m["center"][0] + offset_x, m["center"][1] + offset_y)
        output_result(
            {
                "success": len(matches) > 0,
                "count": len(matches),
                "matches": matches,
                "message": f"Found {len(matches)} match(es)",
            },
            args.json,
        )
    else:
        match = ImageSearch.find_template(args.template, source, threshold=args.threshold)
        if match["found"]:
            match["x"] += offset_x
            match["y"] += offset_y
            match["center"] = (match["center"][0] + offset_x, match["center"][1] + offset_y)
            match["success"] = True
            match["message"] = f"Template found at center {match['center']} with score {match['score']}"
        else:
            match["success"] = False
            match["error"] = f"Template not found (best score: {match.get('score', 0)})"
        output_result(match, args.json)


def cmd_click(args):
    if args.coords:
        x, y = args.coords
    elif args.template:
        match = ImageSearch.wait_for_template(
            args.template,
            timeout=args.timeout,
            threshold=args.threshold,
            window_title=args.window,
        )
        if not match.get("found"):
            output_result(
                {"success": False, "error": f"Template '{args.template}' not found within timeout"},
                args.json,
            )
            sys.exit(1)
        x, y = match["center"]
    else:
        output_result({"success": False, "error": "Either --coords or --template is required"}, args.json)
        sys.exit(1)

    if args.offset:
        x += args.offset[0]
        y += args.offset[1]

    InputController.click(x, y, button=args.button, clicks=args.clicks)
    output_result(
        {
            "success": True,
            "clicked": {"x": x, "y": y, "button": args.button, "clicks": args.clicks},
            "message": f"Clicked at ({x}, {y})",
        },
        args.json,
    )


def cmd_hover(args):
    if args.coords:
        x, y = args.coords
    elif args.template:
        match = ImageSearch.wait_for_template(
            args.template,
            timeout=args.timeout,
            threshold=args.threshold,
            window_title=args.window,
        )
        if not match.get("found"):
            output_result(
                {"success": False, "error": f"Template '{args.template}' not found within timeout"},
                args.json,
            )
            sys.exit(1)
        x, y = match["center"]
    else:
        output_result({"success": False, "error": "Either --coords or --template is required"}, args.json)
        sys.exit(1)

    InputController.hover(x, y, duration=args.duration)
    output_result(
        {
            "success": True,
            "hovered": {"x": x, "y": y},
            "message": f"Hovered at ({x}, {y})",
        },
        args.json,
    )


def cmd_type_text(args):
    InputController.type_text(args.text)
    output_result({"success": True, "typed": args.text, "message": f"Typed text ({len(args.text)} chars)"}, args.json)


def cmd_hotkey(args):
    InputController.send_hotkey(*args.keys)
    output_result({"success": True, "keys": args.keys, "message": f"Pressed hotkey {'+'.join(args.keys)}"}, args.json)


def cmd_copy_to_clipboard(args):
    if args.image:
        res = ScreenCapture.copy_image_to_clipboard(args.image)
        output_result(
            {"success": res, "source": str(args.image), "message": "Copied image file to clipboard"},
            args.json,
        )
    elif args.window:
        win = WindowManager.find_window(args.window)
        if not win:
            output_result({"success": False, "error": f"Window '{args.window}' not found"}, args.json)
            sys.exit(1)
        img = ScreenCapture.capture_window(win, bring_to_front=True)
        res = ScreenCapture.copy_image_to_clipboard(img)
        output_result(
            {"success": res, "window": win.title, "message": f"Copied window '{win.title}' to clipboard"},
            args.json,
        )
    else:
        img = ScreenCapture.capture_screen()
        res = ScreenCapture.copy_image_to_clipboard(img)
        output_result(
            {"success": res, "message": "Copied full screen to clipboard"},
            args.json,
        )


def cmd_diff(args):
    res = VisualDiff.compare(
        args.baseline,
        args.current,
        diff_output_path=args.diff_out,
        tolerance=args.tolerance,
    )
    res["success"] = True
    output_result(res, args.json)


def cmd_launch(args):
    session = AppSession(args.cmd, working_dir=args.cwd)
    started = session.start(wait_secs=args.wait, window_title=args.window)
    data = {
        "success": started,
        "pid": session.process.pid if session.process else None,
        "window": session.window_info.to_dict() if session.window_info else None,
        "message": f"Application launched (PID: {session.process.pid if session.process else 'unknown'})",
    }
    output_result(data, args.json)


def main():
    common_parser = argparse.ArgumentParser(add_help=False)
    common_parser.add_argument("--json", action="store_true", help="Format output as JSON for AI agents or scripts")

    parser = argparse.ArgumentParser(
        description="GUI Tester CLI - Visual Automation, Template Matching, and Screen Inspection",
        parents=[common_parser]
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    # list-windows
    p_list = subparsers.add_parser("list-windows", parents=[common_parser], help="List visible top-level windows")
    p_list.add_argument("--filter", "-f", type=str, default=None, help="Filter by title substring")
    p_list.add_argument("--all", action="store_true", help="Include invisible windows")
    p_list.set_defaults(func=cmd_list_windows)

    # focus
    p_focus = subparsers.add_parser("focus", parents=[common_parser], help="Bring window to front")
    p_focus.add_argument("--window", "-w", required=True, help="Window title substring")
    p_focus.set_defaults(func=cmd_focus_window)

    # capture
    p_cap = subparsers.add_parser("capture", parents=[common_parser], help="Take screenshot of screen, window, or region")
    p_cap.add_argument("--window", "-w", type=str, default=None, help="Window title substring")
    p_cap.add_argument("--rect", "-r", type=int, nargs=4, metavar=("X", "Y", "W", "H"), help="Crop rect")
    p_cap.add_argument("--output", "-o", type=str, default=None, help="Save path (.png)")
    p_cap.add_argument("--to-clipboard", "-c", action="store_true", help="Also copy image to clipboard")
    p_cap.add_argument("--no-focus", action="store_true", help="Do not force window to front before capture")
    p_cap.set_defaults(func=cmd_capture)

    # find-image
    p_find = subparsers.add_parser("find-image", parents=[common_parser], help="Search for template image on screen/window")
    p_find.add_argument("--template", "-t", required=True, help="Template image path")
    p_find.add_argument("--window", "-w", type=str, default=None, help="Window title substring")
    p_find.add_argument("--source", "-s", type=str, default=None, help="Source image path instead of live screen")
    p_find.add_argument("--threshold", type=float, default=0.8, help="Confidence threshold (0.0 - 1.0)")
    p_find.add_argument("--all", action="store_true", help="Find all matches instead of best")
    p_find.set_defaults(func=cmd_find_image)

    # click
    p_click = subparsers.add_parser("click", parents=[common_parser], help="Click coordinates or found template")
    p_click.add_argument("--coords", "-c", type=int, nargs=2, metavar=("X", "Y"), help="Screen coordinates")
    p_click.add_argument("--template", "-t", type=str, help="Template image path to click center of")
    p_click.add_argument("--window", "-w", type=str, default=None, help="Window title when searching template")
    p_click.add_argument("--button", "-b", choices=["left", "right", "middle"], default="left")
    p_click.add_argument("--clicks", type=int, default=1, help="Click count (1=single, 2=double)")
    p_click.add_argument("--offset", type=int, nargs=2, default=(0, 0), metavar=("OX", "OY"))
    p_click.add_argument("--timeout", type=float, default=5.0, help="Wait timeout for template")
    p_click.add_argument("--threshold", type=float, default=0.8)
    p_click.set_defaults(func=cmd_click)

    # hover
    p_hover = subparsers.add_parser("hover", parents=[common_parser], help="Move mouse to trigger hover state/tooltip")
    p_hover.add_argument("--coords", "-c", type=int, nargs=2, metavar=("X", "Y"))
    p_hover.add_argument("--template", "-t", type=str)
    p_hover.add_argument("--window", "-w", type=str, default=None)
    p_hover.add_argument("--duration", type=float, default=0.2)
    p_hover.add_argument("--timeout", type=float, default=5.0)
    p_hover.add_argument("--threshold", type=float, default=0.8)
    p_hover.set_defaults(func=cmd_hover)

    # type-text
    p_type = subparsers.add_parser("type-text", parents=[common_parser], help="Type text string")
    p_type.add_argument("--text", "-t", required=True)
    p_type.set_defaults(func=cmd_type_text)

    # hotkey
    p_hk = subparsers.add_parser("hotkey", parents=[common_parser], help="Send hotkey combination")
    p_hk.add_argument("--keys", "-k", nargs="+", required=True, help="e.g. ctrl v or alt f")
    p_hk.set_defaults(func=cmd_hotkey)

    # copy-to-clipboard
    p_clip = subparsers.add_parser("copy-to-clipboard", parents=[common_parser], help="Copy image/screen to Windows clipboard")
    p_clip.add_argument("--image", "-i", type=str, help="Image file path to put in clipboard")
    p_clip.add_argument("--window", "-w", type=str, help="Capture this window directly into clipboard")
    p_clip.set_defaults(func=cmd_copy_to_clipboard)

    # diff
    p_diff = subparsers.add_parser("diff", parents=[common_parser], help="Compare two images and highlight differences")
    p_diff.add_argument("--baseline", "-b", required=True, help="Baseline reference image")
    p_diff.add_argument("--current", "-c", required=True, help="Current image to verify")
    p_diff.add_argument("--diff-out", "-d", type=str, default=None, help="Output path for highlighted diff image")
    p_diff.add_argument("--tolerance", type=float, default=0.01, help="Allowed difference ratio")
    p_diff.set_defaults(func=cmd_diff)

    # launch
    p_launch = subparsers.add_parser("launch", parents=[common_parser], help="Launch application process")
    p_launch.add_argument("--cmd", required=True, help="Executable path or command")
    p_launch.add_argument("--cwd", default=None, help="Working directory")
    p_launch.add_argument("--wait", type=float, default=2.0, help="Initial wait time in seconds")
    p_launch.add_argument("--window", type=str, default=None, help="Window title substring to wait for")
    p_launch.set_defaults(func=cmd_launch)

    parsed = parser.parse_args()
    parsed.func(parsed)


if __name__ == "__main__":
    main()
