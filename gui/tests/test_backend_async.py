"""Pruebas de BackendQueue / RetryBook sin display (KBC-08/09/41)."""
from __future__ import annotations

import threading
import time
import unittest

import gui.backend as backend
from gui.backend import BackendQueue, Intent, RetryBook, RetryIntent


class FakeClient:
    def __init__(self, hold_kinds=None):
        self.calls: list[tuple[str, object]] = []
        self._hold = set(hold_kinds or ())
        self._gates: dict[str, threading.Event] = {}
        self._entered: dict[str, threading.Event] = {}

    def _gate(self, kind: str) -> None:
        entered = self._entered.setdefault(kind, threading.Event())
        entered.set()
        if kind in self._hold:
            gate = self._gates.setdefault(kind, threading.Event())
            if not gate.wait(timeout=5):
                raise TimeoutError(f"gate {kind}")

    def set_brightness(self, v):
        self._gate("brightness")
        self.calls.append(("brightness", v))

    def set_color(self, v):
        self._gate("set")
        self.calls.append(("set", v))

    def stop(self):
        self._gate("stop")
        self.calls.append(("stop", None))

    def off(self):
        self._gate("off")
        self.calls.append(("off", None))

    def animation(self, mode):
        self._gate("animation")
        self.calls.append(("animation", mode))

    def restore(self):
        self._gate("restore")
        self.calls.append(("restore", None))


class TestRetryBook(unittest.TestCase):
    def test_off_antiguo_no_anula_set_nuevo(self):
        book = RetryBook(max_attempts=3)
        off = book.replace("off")
        self.assertTrue(book.begin(off))
        color = book.replace("set", "ff0000")
        nxt, superseded = book.finish(off, ok=False)
        self.assertTrue(superseded)
        self.assertIsNotNone(nxt)
        assert nxt is not None
        self.assertEqual(nxt.kind, "set")
        self.assertEqual(nxt.payload, "ff0000")
        self.assertEqual(book.current("off"), color)

    def test_reintentos_acotados(self):
        book = RetryBook(max_attempts=2)
        intent = book.replace("brightness", 40)
        self.assertTrue(book.begin(intent))
        nxt, _ = book.finish(intent, ok=False)
        self.assertIsNone(nxt)
        pending = book.current("brightness")
        self.assertIsNotNone(pending)
        assert pending is not None
        self.assertEqual(pending.attempts, 1)
        self.assertTrue(book.begin(pending))
        nxt2, _ = book.finish(pending, ok=False)
        self.assertIsNone(nxt2)
        self.assertIsNone(book.current("brightness"))

    def test_due_omite_inflight(self):
        book = RetryBook()
        a = book.replace("brightness", 1)
        self.assertTrue(book.begin(a))
        self.assertEqual(book.due(), [])
        book.finish(a, ok=False)
        due = book.due()
        self.assertEqual(len(due), 1)
        self.assertEqual(due[0].payload, 1)


class TestBackendQueueKbc41(unittest.TestCase):
    def setUp(self):
        self._prev_glib = backend.GLib
        backend.GLib = None

    def tearDown(self):
        backend.GLib = self._prev_glib

    def test_callback_reentrante_sin_glib_no_deadlock(self):
        """Sustitución con callback que reentra en submit debe terminar a tiempo."""
        client = FakeClient(hold_kinds={"stop"})
        q = BackendQueue(client)
        results: list[tuple[str, bool, str | None]] = []
        done = threading.Event()

        def cb_old(ok, err):
            results.append(("old", ok, err))
            q.submit(
                Intent(
                    "brightness",
                    99,
                    on_done=lambda ok2, err2: (
                        results.append(("reenter", ok2, err2)),
                        done.set(),
                    ),
                )
            )

        # Retener al worker en stop para que brightness quede en pending.
        stop_entered = client._entered.setdefault("stop", threading.Event())
        stop_gate = client._gates.setdefault("stop", threading.Event())
        q.submit(Intent("stop"))
        self.assertTrue(stop_entered.wait(timeout=2), "worker no entró en stop")

        q.submit(Intent("brightness", 10, on_done=cb_old))
        q.submit(
            Intent(
                "brightness",
                20,
                on_done=lambda ok, err: results.append(("new", ok, err)),
            )
        )

        # Sin el arreglo KBC-41 esto deadlockeaba bajo el Lock.
        stop_gate.set()
        self.assertTrue(done.wait(timeout=3), "timeout: posible deadlock reentrante")
        q.close()
        time.sleep(0.05)

        self.assertIn(("old", False, "sustituido"), results)
        kinds = [c[0] for c in client.calls]
        self.assertIn("stop", kinds)
        self.assertIn("brightness", kinds)
        self.assertEqual(client.calls[-1], ("brightness", 99))


class TestBackendQueueConsolidacion(unittest.TestCase):
    def setUp(self):
        self._prev_glib = backend.GLib
        backend.GLib = None

    def tearDown(self):
        backend.GLib = self._prev_glib

    def test_consolida_brillo(self):
        client = FakeClient()
        q = BackendQueue(client)
        done = []
        q.submit(Intent("brightness", 10, lambda ok, e: done.append(1)))
        q.submit(Intent("brightness", 20, lambda ok, e: done.append(2)))
        q.submit(Intent("brightness", 30, lambda ok, e: done.append(3)))
        time.sleep(0.25)
        q.close()
        time.sleep(0.05)
        self.assertEqual([c for c in client.calls if c[0] == "brightness"][-1:], [("brightness", 30)])


class TestRetryIntentInmutable(unittest.TestCase):
    def test_frozen(self):
        intent = RetryIntent("off", None, 0)
        with self.assertRaises(Exception):
            intent.attempts = 2  # type: ignore[misc]


if __name__ == "__main__":
    unittest.main()
