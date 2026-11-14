"""Cola serializada de operaciones CLI fuera del hilo GTK.

Las peticiones de brillo/color se consolidan (solo vale el último valor).
Los callbacks de resultado se despachan fuera del candado (KBC-41) y, con
GLib disponible, vía ``GLib.idle_add``.
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
    from gui import api
except ImportError:
    import os
    import sys

    _here = os.path.dirname(os.path.abspath(__file__))
    _root = os.path.dirname(_here)
    if _root not in sys.path:
        sys.path.insert(0, _root)
    from gui import api

_load, log_exc, _gettext_func = api(__file__)
_ = _gettext_func()


@dataclass(frozen=True)
class Intent:
    kind: str
    payload: Any = None
    on_done: Callable[[bool, str | None], None] | None = None


@dataclass(frozen=True)
class RetryIntent:
    """Intención inmutable para reintentos acotados (KBC-09)."""

    kind: str
    payload: Any = None
    attempts: int = 0


class RetryBook:
    """Consolida intenciones por familia y limita reintentos.

    Las familias ``mode`` (set/animation/off/restore) se anulan entre sí:
    un *off* antiguo no sobrevive a una intención nueva de color o animación.
    """

    MODE_KINDS = frozenset({"set", "animation", "off", "restore"})
    DEFAULT_MAX_ATTEMPTS = 5

    def __init__(self, max_attempts: int = DEFAULT_MAX_ATTEMPTS):
        self._max = max(1, int(max_attempts))
        self._items: dict[str, RetryIntent] = {}
        self._inflight: set[str] = set()

    def key_for(self, kind: str) -> str:
        if kind in self.MODE_KINDS:
            return "mode"
        return kind

    def replace(self, kind: str, payload: Any = None) -> RetryIntent:
        key = self.key_for(kind)
        intent = RetryIntent(kind=kind, payload=payload, attempts=0)
        self._items[key] = intent
        return intent

    def current(self, kind: str) -> RetryIntent | None:
        return self._items.get(self.key_for(kind))

    def begin(self, intent: RetryIntent) -> bool:
        """Marca vuelo; False si ya hay uno en curso para la misma clave."""
        key = self.key_for(intent.kind)
        cur = self._items.get(key)
        if cur is None or (cur.kind, cur.payload) != (intent.kind, intent.payload):
            return False
        if key in self._inflight:
            return False
        self._inflight.add(key)
        return True

    def finish(
        self, intent: RetryIntent, ok: bool
    ) -> tuple[RetryIntent | None, bool]:
        """Cierra el vuelo. Devuelve (siguiente_a_disparar, supersedido).

        Si *ok*, limpia la entrada vigente coincidente.
        Si falla y quedan intentos, incrementa ``attempts`` y deja la entrada
        para el temporizador; si fue sustituida, dispara la vigente.
        """
        key = self.key_for(intent.kind)
        self._inflight.discard(key)
        cur = self._items.get(key)
        if cur is None:
            return None, True
        if (cur.kind, cur.payload) != (intent.kind, intent.payload):
            # Off/color antiguos no anulan la intención nueva.
            return cur, True
        if ok:
            self._items.pop(key, None)
            return None, False
        nxt_attempts = intent.attempts + 1
        if nxt_attempts >= self._max:
            self._items.pop(key, None)
            return None, False
        nxt = RetryIntent(kind=intent.kind, payload=intent.payload, attempts=nxt_attempts)
        self._items[key] = nxt
        return None, False

    def due(self) -> list[RetryIntent]:
        """Intenciones pendientes que no están en vuelo (reintento o tras sustitución)."""
        out: list[RetryIntent] = []
        for key, intent in list(self._items.items()):
            if key in self._inflight:
                continue
            out.append(intent)
        return out

    def clear(self) -> None:
        self._items.clear()
        self._inflight.clear()


class BackendQueue:
    """Un worker; cancela intenciones sustituidas del mismo kind consolidable."""

    _CONSOLIDABLE = frozenset({"brightness", "set"})

    def __init__(self, client):
        self._client = client
        self._q: queue.Queue[Intent | None] = queue.Queue()
        self._pending: dict[str, Intent] = {}
        self._lock = threading.Lock()
        self._closed = False
        self._thread = threading.Thread(target=self._loop, name="ilutex-backend", daemon=True)
        self._thread.start()

    def submit(self, intent: Intent) -> None:
        if self._closed:
            if intent.on_done:
                self._emit(intent.on_done, False, "backend cerrado")
            return
        # KBC-41: no invocar callbacks bajo el candado (deadlock si reentran).
        superseded_cb = None
        with self._lock:
            if intent.kind in self._CONSOLIDABLE:
                old = self._pending.pop(intent.kind, None)
                if old and old.on_done:
                    superseded_cb = old.on_done
                self._pending[intent.kind] = intent
            self._q.put(intent)
        if superseded_cb is not None:
            self._emit(superseded_cb, False, "sustituido")

    def close(self) -> None:
        self._closed = True
        self._q.put(None)

    def join(self, timeout: float | None = None) -> bool:
        """Espera al worker tras ``close``. Devuelve True si el hilo terminó."""
        self._thread.join(timeout=timeout)
        return not self._thread.is_alive()

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
                        return False, _("The animation ended as it started")
            elif k == "off":
                c.off()
            elif k == "restore":
                c.restore()
            elif k == "firmware":
                c.firmware_effects(bool(p))
            else:
                return False, _("unknown intent: {kind}").format(kind=k)
            return True, None
        except Exception as e:
            log_exc(f"backend {intent.kind}")
            return False, getattr(e, "message", None) or str(e)
