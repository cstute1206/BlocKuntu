#!/usr/bin/env python3
"""Map a GTK window in the actual logged-in desktop; no extra Python packages."""
import ctypes
import ctypes.util
import json
import os
import time
from pathlib import Path

gtk = ctypes.CDLL(ctypes.util.find_library("gtk-3"))
gtk.gtk_init_check.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
gtk.gtk_init_check.restype = ctypes.c_int
if not gtk.gtk_init_check(None, None):
    raise SystemExit("GTK cannot connect to the desktop display")
gtk.gtk_window_new.argtypes = [ctypes.c_int]
gtk.gtk_window_new.restype = ctypes.c_void_p
gtk.gtk_window_set_title.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
gtk.gtk_window_set_default_size.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
gtk.gtk_label_new.argtypes = [ctypes.c_char_p]
gtk.gtk_label_new.restype = ctypes.c_void_p
gtk.gtk_container_add.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
gtk.gtk_widget_show_all.argtypes = [ctypes.c_void_p]
gtk.gtk_widget_get_mapped.argtypes = [ctypes.c_void_p]
gtk.gtk_widget_get_mapped.restype = ctypes.c_int
gtk.gtk_widget_destroy.argtypes = [ctypes.c_void_p]
window = gtk.gtk_window_new(0)
gtk.gtk_window_set_title(window, b"BlocKuntu Phase 0 - desktop smoke test")
gtk.gtk_window_set_default_size(window, 640, 240)
gtk.gtk_container_add(window, gtk.gtk_label_new(b"Phase 0: GUI started through SSH\nDisposable test clone"))
gtk.gtk_widget_show_all(window)
deadline = time.monotonic() + 45
ready = False
while time.monotonic() < deadline:
    while gtk.gtk_events_pending():
        gtk.gtk_main_iteration_do(False)
    if not ready and gtk.gtk_widget_get_mapped(window):
        Path("gui-ready.json").write_text(json.dumps({
            "mapped": True, "uid": os.getuid(), "pid": os.getpid(),
            "display": os.environ.get("DISPLAY"),
            "wayland_display": os.environ.get("WAYLAND_DISPLAY"),
        }) + "\n")
        ready = True
    time.sleep(0.05)
gtk.gtk_widget_destroy(window)
if not ready:
    raise SystemExit("GTK window never became mapped")
