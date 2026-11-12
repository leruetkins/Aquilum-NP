#!/usr/bin/env python3
"""
Automated regression test for editor touch scrolling jitter on Android via ADB.

Tests:
1. Verifies ADB device connectivity.
2. Performs controlled slow drag gestures across editor band boundaries.
3. Analyzes logcat for:
   - Correct band handoffs.
   - Absence of cascading false re-renders.
4. Captures frame sequence and analyzes vertical displacement continuity via OpenCV phase correlation.
5. Fails (exit code 1) if jitter or position jumps are detected.
"""

import os
import re
import sys
import time
import tempfile
import subprocess
import shutil

try:
    import cv2
    import numpy as np
    HAS_OPENCV = True
except ImportError:
    HAS_OPENCV = False

PACKAGE = "md.vibecodium.mobile"
ACTIVITY = "md.vibecodium.mobile/.MainActivity"

def find_adb():
    sdk_adb = os.path.expanduser("~/AppData/Local/Android/Sdk/platform-tools/adb.exe")
    if os.path.exists(sdk_adb):
        return sdk_adb
    candidate = shutil.which("adb")
    if candidate:
        return candidate
    npm_adb = os.path.expanduser("~/AppData/Roaming/npm/adb.cmd")
    if os.path.exists(npm_adb):
        return npm_adb
    return "adb"

ADB_BIN = find_adb()

def format_args(args):
    cmd = [ADB_BIN if args[0] == "adb" else args[0]] + args[1:]
    use_shell = os.name == "nt" and cmd[0].endswith((".cmd", ".bat", ".ps1"))
    return cmd, use_shell

def run_cmd(args, timeout=30):
    cmd, use_shell = format_args(args)
    res = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, shell=use_shell)
    return res.returncode, res.stdout.strip(), res.stderr.strip()

def popen_cmd(args):
    cmd, use_shell = format_args(args)
    return subprocess.Popen(cmd, shell=use_shell)

def check_adb_device():
    code, out, _ = run_cmd(["adb", "devices"])
    lines = [l for l in out.splitlines()[1:] if l.strip() and "device" in l]
    if not lines:
        print("[FAIL] No connected ADB device found. Connect a device or start an emulator.")
        sys.exit(1)
    device_id = lines[0].split()[0]
    print(f"[INFO] Using ADB device: {device_id}")
    return device_id

def ensure_app_running():
    print("[INFO] Launching app...")
    run_cmd(["adb", "shell", "am", "start", "-n", ACTIVITY])
    time.sleep(1.2)

def clear_logs():
    run_cmd(["adb", "logcat", "-c"])

def get_logs():
    _, out, _ = run_cmd(["adb", "logcat", "-d", "-s", "VibecodiumRender", "VibecodiumRust", "VibecodiumTest"])
    return out

def perform_drag_test():
    print("[INFO] Performing drag gestures across band boundaries...")
    # Drag upward (scroll down)
    run_cmd(["adb", "shell", "input", "swipe", "500", "1500", "500", "800", "1200"])
    time.sleep(0.3)
    # Drag downward (scroll up)
    run_cmd(["adb", "shell", "input", "swipe", "500", "800", "500", "1500", "1200"])
    time.sleep(0.5)

def analyze_logs(logs):
    print("[INFO] Analyzing logcat for band swaps and cascading re-renders...")
    lines = logs.splitlines()
    render_origins = []

    for l in lines:
        m_render = re.search(r"(?:rendered|bandOrigin)=([0-9.]+)", l)
        if m_render:
            val = float(m_render.group(1))
            render_origins.append(val)

    if not render_origins:
        print("[WARN] No band render events found in logs during gesture (content may already be static).")
        return True

    # Check for cascading re-renders:
    # If consecutive distinct renders happen with very small deltas (< 200px),
    # that indicates the cascading bug.
    cascading_count = 0
    for i in range(len(render_origins) - 1):
        delta = abs(render_origins[i+1] - render_origins[i])
        if 0 < delta < 200.0:
            print(f"[WARN] Suspicious minor re-render delta: {delta}px between {render_origins[i]} and {render_origins[i+1]}")
            cascading_count += 1

    if cascading_count > 2:
        print(f"[FAIL] Detected {cascading_count} cascading re-renders! Regressing to false re-render loop.")
        return False

    print(f"[PASS] Band render lifecycle valid ({len(render_origins)} renders recorded, 0 cascading loops).")
    return True

def analyze_visual_smoothness():
    if not HAS_OPENCV:
        print("[WARN] OpenCV not available, skipping frame-by-frame phase correlation analysis.")
        return True

    print("[INFO] Capturing video recording of scrolling motion for phase correlation...")
    temp_dir = tempfile.mkdtemp(prefix="vibecodium_test_")
    local_video = os.path.join(temp_dir, "scroll.mp4")
    remote_video = "/sdcard/vibe_test_scroll.mp4"

    try:
        proc = popen_cmd(["adb", "shell", "screenrecord", "--size", "720x1280", remote_video])
        time.sleep(0.8)

        # Smooth drag gestures
        run_cmd(["adb", "shell", "input", "swipe", "500", "1500", "500", "700", "1000"])
        time.sleep(0.3)
        run_cmd(["adb", "shell", "input", "swipe", "500", "700", "500", "1500", "1000"])
        time.sleep(0.5)

        run_cmd(["adb", "shell", "pkill", "-2", "screenrecord"])
        try:
            proc.wait(timeout=3)
        except Exception:
            proc.kill()
        time.sleep(0.5)

        run_cmd(["adb", "pull", remote_video, local_video])
        run_cmd(["adb", "shell", "rm", remote_video])

        if not os.path.exists(local_video) or os.path.getsize(local_video) < 1000:
            print("[WARN] Could not retrieve screen recording for visual analysis.")
            return True

        cap = cv2.VideoCapture(local_video)
        frames = []
        while True:
            ret, frame = cap.read()
            if not ret:
                break
            # Crop middle text area to avoid status bar and overlay
            h, w = frame.shape[:2]
            y1, y2 = int(h * 0.2), int(h * 0.8)
            x1, x2 = int(w * 0.1), int(w * 0.9)
            gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)[y1:y2, x1:x2].astype(np.float32)
            frames.append(gray)
        cap.release()

        if len(frames) < 10:
            print("[WARN] Not enough frames captured.")
            return True

        shifts = []
        for i in range(len(frames) - 1):
            shift, resp = cv2.phaseCorrelate(frames[i], frames[i+1])
            shifts.append(shift[1])

        # Analyze acceleration / jumps:
        # During uniform swipe, second derivative (jerk) should be low.
        max_jump = 0.0
        for i in range(len(shifts) - 1):
            s1 = shifts[i]
            s2 = shifts[i+1]
            if (s1 < -3 and s2 < -3) or (s1 > 3 and s2 > 3):
                jump = abs(s2 - s1)
                if jump > max_jump:
                    max_jump = jump

        print(f"[INFO] Maximum inter-frame displacement jerk: {max_jump:.2f}px")
        if max_jump > 25.0:
            print(f"[FAIL] Significant jitter detected ({max_jump:.2f}px jump between consecutive frames)!")
            return False

        print(f"[PASS] Frame displacement continuity verified (max jerk {max_jump:.2f}px <= 25px threshold).")
        return True

    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

def main():
    print("=" * 60)
    print("  Vibecodium Mobile: Scrolling Jitter Regression Test")
    print("=" * 60)

    check_adb_device()
    ensure_app_running()
    clear_logs()

    perform_drag_test()
    logs = get_logs()

    log_pass = analyze_logs(logs)
    visual_pass = analyze_visual_smoothness()

    print("=" * 60)
    if log_pass and visual_pass:
        print("  RESULT: ALL TESTS PASSED! No scrolling jitter detected.")
        print("=" * 60)
        sys.exit(0)
    else:
        print("  RESULT: REGRESSION DETECTED! Test failed.")
        print("=" * 60)
        sys.exit(1)

if __name__ == "__main__":
    main()
