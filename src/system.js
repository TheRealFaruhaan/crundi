/**
 * system.js — Window listing, screenshot capture, and virtual desktop support (Windows)
 * Uses node-screenshots (Windows Graphics Capture API) for display/window capture.
 * Window enumeration uses IVirtualDesktopManager COM to list across all virtual desktops.
 * Virtual desktop switching uses IVirtualDesktopManagerInternal COM (with keyboard fallback).
 */

import { execFile, execSync, execFileSync } from 'child_process';
import { Window, Monitor as Screen } from 'node-screenshots';
import { IS_WIN } from './platform.js';

// ─── List all open windows (across all virtual desktops) ───

const SKIP_TITLES = new Set(['', 'Program Manager', 'Windows Input Experience']);

let windowCache = [];
let windowCacheTime = null;

// PowerShell script that uses the documented IVirtualDesktopManager COM interface
// to enumerate windows across ALL virtual desktops (EnumWindows returns all handles,
// GetWindowDesktopId tags each with its desktop GUID).
const PS_ENUM_ALL_WINDOWS = `
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
using System.Text;

[ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("a5cd92ff-29be-454c-8d04-d82879fb3f1b")]
public interface IVirtualDesktopManager {
    bool IsWindowOnCurrentVirtualDesktop(IntPtr topLevelWindow);
    Guid GetWindowDesktopId(IntPtr topLevelWindow);
    void MoveWindowToDesktop(IntPtr topLevelWindow, ref Guid desktopId);
}

public class VDEnumerator {
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc cb, IntPtr p);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll", CharSet=CharSet.Auto)] static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);

    delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    public static string GetAllWindows() {
        var mgr = (IVirtualDesktopManager)Activator.CreateInstance(
            Type.GetTypeFromCLSID(new Guid("aa509086-5ca9-4c25-8f95-589d3c07b48a")));
        var results = new List<string>();
        EnumWindows((hWnd, param) => {
            if (!IsWindowVisible(hWnd)) return true;
            var sb = new StringBuilder(512);
            GetWindowText(hWnd, sb, 512);
            string title = sb.ToString();
            if (string.IsNullOrWhiteSpace(title)) return true;
            try {
                Guid deskId = mgr.GetWindowDesktopId(hWnd);
                if (deskId == Guid.Empty) return true;
                bool isCurrent = mgr.IsWindowOnCurrentVirtualDesktop(hWnd);
                uint pid; GetWindowThreadProcessId(hWnd, out pid);
                results.Add(hWnd.ToInt64() + "\\t" + title + "\\t" + deskId + "\\t" + (isCurrent ? "1" : "0") + "\\t" + pid);
            } catch {}
            return true;
        }, IntPtr.Zero);
        return string.Join("\\n", results);
    }
}
"@
[VDEnumerator]::GetAllWindows()
`.trim();

/**
 * List all open windows across all virtual desktops.
 * Uses COM IVirtualDesktopManager for cross-desktop enumeration, then enriches
 * with metadata from node-screenshots (appName, isFocused, isMinimized) for
 * windows on the current desktop.
 */
export function listWindows() {
  const locked = isScreenLocked();

  // Get node-screenshots data for current-desktop windows (has appName, focus, etc.)
  let nsWindows;
  try {
    nsWindows = Window.all()
      .filter(w => {
        const t = w.title();
        return t && !SKIP_TITLES.has(t) && !t.startsWith('Default IME');
      })
      .map(w => ({
        id: w.id(),
        hwnd: w.id(),
        title: w.title(),
        appName: w.appName(),
        isFocused: w.isFocused(),
        isMinimized: w.isMinimized(),
      }));
  } catch {
    nsWindows = [];
  }
  const nsMap = new Map(nsWindows.map(w => [w.hwnd, w]));

  // Get COM-based enumeration (all virtual desktops) — Windows-only.
  // On other platforms node-screenshots already returns current-Space windows,
  // so comWindows stays empty and we fall back to nsWindows below.
  let comWindows = [];
  if (IS_WIN) try {
    const raw = execFileSync(
      'powershell.exe',
      ['-NoProfile', '-NonInteractive', '-WindowStyle', 'Hidden', '-Command', PS_ENUM_ALL_WINDOWS],
      { encoding: 'utf8', timeout: 15000, windowsHide: true }
    ).trim();

    if (raw) {
      // Collect unique desktop GUIDs in order of first appearance
      const desktopOrder = [];
      const desktopSet = new Set();

      for (const line of raw.split('\n')) {
        const parts = line.split('\t');
        if (parts.length < 5) continue;
        const desktopGuid = parts[2];
        if (!desktopSet.has(desktopGuid)) {
          desktopSet.add(desktopGuid);
          desktopOrder.push(desktopGuid);
        }
      }

      // Sort desktops: current first, then others in discovery order
      const currentGuids = new Set();
      for (const line of raw.split('\n')) {
        const parts = line.split('\t');
        if (parts.length >= 4 && parts[3] === '1') currentGuids.add(parts[2]);
      }

      // Assign 1-based desktop numbers
      const desktopNumbers = new Map();
      // Current desktop(s) first
      let num = 1;
      for (const guid of desktopOrder) {
        if (currentGuids.has(guid)) { desktopNumbers.set(guid, num++); }
      }
      for (const guid of desktopOrder) {
        if (!currentGuids.has(guid)) { desktopNumbers.set(guid, num++); }
      }

      for (const line of raw.split('\n')) {
        const parts = line.split('\t');
        if (parts.length < 5) continue;
        const hwnd = parseInt(parts[0], 10);
        const title = parts[1];
        const desktopGuid = parts[2];
        const isCurrent = parts[3] === '1';

        if (SKIP_TITLES.has(title) || title.startsWith('Default IME')) continue;

        // Merge with node-screenshots data if available (current desktop only)
        const ns = nsMap.get(hwnd);
        comWindows.push({
          id: hwnd,
          hwnd,
          title: ns?.title || title,
          appName: ns?.appName || '',
          isFocused: ns?.isFocused || false,
          isMinimized: ns?.isMinimized || false,
          desktop: desktopNumbers.get(desktopGuid) || 0,
          isCurrentDesktop: isCurrent,
        });
      }
    }
  } catch (err) {
    // COM enumeration failed — fall back to node-screenshots only
    console.error('[system] COM window enumeration failed:', err.message);
  }

  // Use COM results if available, otherwise fall back to node-screenshots
  const windows = comWindows.length > 0
    ? comWindows
    : nsWindows.map(w => ({ ...w, desktop: 1, isCurrentDesktop: true }));

  if (!locked) {
    windowCache = windows.length > 0 ? windows : windowCache;
    if (windows.length > 0) windowCacheTime = new Date();
    return { windows, cached: false, cachedAt: windowCacheTime };
  }

  // Locked — return cache if available
  if (windowCache.length > 0) {
    return { windows: windowCache, cached: true, cachedAt: windowCacheTime };
  }

  return { windows, cached: false, cachedAt: null };
}

function isScreenLocked() {
  // Locked-screen detection relies on the Windows LogonUI process; on other
  // platforms we don't keep a locked-screen cache, so report "not locked".
  if (!IS_WIN) return false;
  try {
    const result = execSync(
      'powershell -NoProfile -NonInteractive -WindowStyle Hidden -Command "Get-Process -Name LogonUI -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object { echo locked }"',
      { encoding: 'utf8', timeout: 5000, windowsHide: true }
    ).trim();
    return result === 'locked';
  } catch {
    return false;
  }
}

// ─── List all displays ───

export function listScreens() {
  return Screen.all().map((s, i) => ({
    index: i + 1,
    id: s.id(),
    primary: s.isPrimary(),
    width: s.width(),
    height: s.height(),
    x: s.x(),
    y: s.y(),
    scaleFactor: s.scaleFactor(),
  }));
}

// ─── Screenshot of a display by 1-based index ───
// Returns a Buffer (PNG)

export async function screenshotDisplay(displayIndex = 1) {
  const screens = Screen.all();
  const idx = Number(displayIndex) - 1;
  const screen = screens[idx] || screens.find(s => s.isPrimary()) || screens[0];
  if (!screen) throw new Error('No displays found');
  const image = await screen.captureImage();
  return image.toPng();
}

// ─── Screenshot of a specific window by id ───
// Uses WGC via node-screenshots for current-desktop windows.
// Falls back to PrintWindow Win32 API for cross-desktop windows.
// Returns a Buffer (PNG)

// PrintWindow fallback for windows on other virtual desktops.
// node-screenshots' Window.all() only returns current-desktop windows,
// so we use the Win32 PrintWindow API which works for any hwnd.
function buildPrintWindowScript(hwnd, outPath) {
  return `
Add-Type @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public class WinCapture {
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr hWnd, IntPtr hDC, uint nFlags);
    [DllImport("user32.dll")] static extern bool IsWindow(IntPtr hWnd);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr hWnd, int attr, out RECT rect, int size);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    public static void Capture(long hwndLong, string path) {
        IntPtr hWnd = new IntPtr(hwndLong);
        if (!IsWindow(hWnd)) throw new Exception("Window not found");

        RECT rect;
        int hr = DwmGetWindowAttribute(hWnd, 9, out rect, Marshal.SizeOf(typeof(RECT)));
        if (hr != 0) GetWindowRect(hWnd, out rect);

        int w = rect.Right - rect.Left;
        int h = rect.Bottom - rect.Top;
        if (w <= 0 || h <= 0) throw new Exception("Window has zero size");

        using (var bmp = new Bitmap(w, h)) {
            using (var g = Graphics.FromImage(bmp)) {
                IntPtr hdc = g.GetHdc();
                PrintWindow(hWnd, hdc, 2);
                g.ReleaseHdc(hdc);
            }
            bmp.Save(path, ImageFormat.Png);
        }
    }
}
"@ -ReferencedAssemblies System.Drawing
[WinCapture]::Capture(${hwnd}, '${outPath.replace(/'/g, "''")}')
`.trim();
}

export async function screenshotWindow(windowId) {
  const id = Number(windowId);

  // Try node-screenshots first (current desktop, higher quality via WGC)
  const all = Window.all();
  const win = all.find(w => w.id() === id);
  if (win) {
    const image = await win.captureImage();
    return image.toPng();
  }

  // The PrintWindow fallback below is Windows-only and exists to reach windows on
  // other virtual desktops. On macOS/Linux node-screenshots only sees the current
  // Space, so if we got here the window isn't capturable — fail with a clear message.
  if (!IS_WIN) {
    throw new Error('Window not found on the current Space (cross-desktop capture is Windows-only)');
  }

  // Fallback: PrintWindow via PowerShell (works cross-desktop)
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const { readFileSync, unlinkSync } = await import('node:fs');

  const tmpFile = join(tmpdir(), `wincap-${id}-${Date.now()}.png`);
  try {
    await runPS(buildPrintWindowScript(id, tmpFile));
    const buf = readFileSync(tmpFile);
    return buf;
  } finally {
    try { unlinkSync(tmpFile); } catch {}
  }
}

// ─── Helper: run PowerShell hidden ───

function runPS(script) {
  return new Promise((resolve, reject) => {
    execFile(
      'powershell.exe',
      ['-NoProfile', '-NonInteractive', '-WindowStyle', 'Hidden', '-Command', script],
      { maxBuffer: 10 * 1024 * 1024, windowsHide: true },
      (err, stdout, stderr) => {
        if (err) return reject(new Error(stderr || err.message));
        resolve(stdout);
      }
    );
  });
}
