"""Cola serializada de operaciones CLI fuera del hilo GTK.

Las peticiones de brillo/color se consolidan (solo vale el último valor).
Los callbacks de resultado se despachan con ``GLib.idle_add``.
"""
from __future__ import annotations

import queue
import threading
from dataclasses import dataclass
from typing import Any, Callable

try:
    from gi.repository import GLib
except ImportError:  # tests sin GI
    GLib = None  # type: ignore

try:
    from gui import log_exc
except ImportError:
    def log_exc(contexto):
        import sys
        tipo, exc, _tb = sys.exc_info()
        print(f"keybackcon-gui: {contexto}: {tipo.__name__}: {exc}", file=sys.stderr)


@dataclass(frozen=True)
class Intent:
    kind: str
    payload: Any = None
    on_done: Callable[[bool, str | None], None] | None = None


class BackendQueue:
    """Un worker; cancela intenciones sustituidas del mismo kind consolidable."""

    _CONSOLIDABLE = frozenset({"brightness", "set"})

    def __init__(self, client):
        self._client = client
        self._q: queue.Queue[Intent | None] = queue.Queue()
        self._pending: dict[str, Intent] = {}
        self._lock = threading.Lock()
        self._closed = False
        self._thread = threading.Thread(target=self._loop, name="keybackcon-backend", daemon=True)
        self._thread.start()

    def submit(self, intent: Intent) -> None:
        if self._closed:
            if intent.on_done:
                self._emit(intent.on_done, False, "backend cerrado")
            return
        with self._lock:
            if intent.kind in self._CONSOLIDABLE:
                old = self._pending.pop(intent.kind, None)
                if old and old.on_done:
                    self._emit(old.on_done, False, "sustituido")
                self._pending[intent.kind] = intent
            self._q.put(intent)

    def close(self) -> None:
        self._closed = True
        self._q.put(None)

    def _emit(self, cb, ok: bool, err: str | None) -> None:
        if GLib is None:
            try:
                cb(ok, err)
            except Exception:
                log_exc("callback backend")
            return
        def _run():
            try:
                cb(ok, err)
            except Exception:
                log_exc("callback backend")
            return False
        GLib.idle_add(_run)

    def _loop(self) -> None:
        while True:
            item = self._q.get()
            if item is None:
                return
            with self._lock:
                if item.kind in self._CONSOLIDABLE:
                    current = self._pending.get(item.kind)
                    if current is not item:
                        continue
                    self._pending.pop(item.kind, None)
            ok, err = self._execute(item)
            if item.on_done:
                self._emit(item.on_done, ok, err)

    def _execute(self, intent: Intent) -> tuple[bool, str | None]:
        try:
            c = self._client
            k = intent.kind
            p = intent.payload
            if k == "set":
                c.set_color(p)
            elif k == "brightness":
                c.set_brightness(p)
            elif k == "stop":
                c.stop()
            elif k == "animation":
                c.animation(p)
                if hasattr(c, "_proc") and c._proc is not None:
                    if c._proc.poll() is not None:
                        return False, "la animación terminó al arrancar"
            elif k == "off":
                c.off()
            elif k == "restore":
                c.restore()
            elif k == "firmware":
                c.firmware_effects(bool(p))
            else:
                return False, f"intento desconocido: {k}"
            return True, None
        except Exception as e:
            log_exc(f"backend {intent.kind}")
            return False, getattr(e, "message", None) or str(e)
