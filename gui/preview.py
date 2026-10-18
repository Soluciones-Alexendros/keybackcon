import math
import random
import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, GLib
try:
    from gui.i18n import _
except ImportError:
    try:
        from .i18n import _
    except ImportError:
        try:
            from i18n import _
        except ImportError:
            def _(s):
                return s
try:
    from gui.colors import hex_to_rgb, scale_rgb, hsv_to_rgb
except ImportError:
    try:
        from .colors import hex_to_rgb, scale_rgb, hsv_to_rgb
    except ImportError:
        from colors import hex_to_rgb, scale_rgb, hsv_to_rgb


MODES = ("fijar", "breathe", "rainbow")

_MAX_PARTICLES = 120
_BREATHE_PERIOD = 4.0
_FIX_PULSE_PERIOD = 3.0
_RAINBOW_DEG_PER_SEC = 45.0
_DT_MAX = 0.05
_BREATHE_RATE_BASE = 8.0
_BREATHE_RATE_SPAN = 30.0
_RAINBOW_RATE = 42.0


def animations_enabled():
    try:
        settings = Gtk.Settings.get_default()
        return bool(settings.get_property("gtk-enable-animations"))
    except Exception:
        return True


def mode_label(mode):
    if mode == "breathe":
        return _("Breathe")
    if mode == "rainbow":
        return _("Rainbow")
    return _("Steady")


def resolve_color(color_hex, pct, mode, t, animate):
    if mode == "rainbow" and animate:
        r, g, b = hsv_to_rgb((t * 45.0) % 360.0, 1.0, 1.0)
        k = pct / 100.0
        return r * k, g * k, b * k
    if mode == "breathe" and animate:
        k = (1 - math.cos(t * 2 * math.pi / 4.0)) / 2.0
        factor = (8.0 + k * 92.0) * (pct / 100.0) / 100.0
        br, bg, bb = hex_to_rgb(color_hex)
        return br * factor, bg * factor, bb * factor
    return scale_rgb(color_hex, pct)


def draw_stage(area, ctx, w, h, *, color_hex, pct, mode, t0):
    ctx.set_source_rgba(0, 0, 0, 0)
    ctx.paint()
    t = (GLib.get_monotonic_time() - t0) / 1000000.0
    animate = animations_enabled()
    r, g, b = resolve_color(color_hex, pct, mode, t, animate)
    cx, cy = w / 2.0, h / 2.0 - 8
    for i, alpha in ((64, 0.10), (48, 0.16), (34, 0.28)):
        ctx.set_source_rgba(r, g, b, alpha)
        ctx.arc(cx, cy, i, 0, 2 * math.pi)
        ctx.fill()
    ctx.set_source_rgb(r, g, b)
    ctx.set_line_width(3)
    ctx.rectangle(cx - 72, cy - 22, 144, 44)
    ctx.stroke()
    for i in range(3):
        for j in range(8):
            x = cx - 60 + j * 17
            y = cy - 10 + i * 11
            ctx.rectangle(x, y, 10, 5)
            ctx.fill()
    try:
        fg = area.get_style_context().get_color()
        ctx.set_source_rgba(fg.red, fg.green, fg.blue, 0.75)
    except Exception:
        ctx.set_source_rgba(0.95, 0.94, 0.90, 0.75)
    ctx.set_font_size(11)
    ctx.move_to(12, h - 12)
    try:
        ctx.show_text(f"#{color_hex} · {pct}% · {mode_label(mode)}")
    except Exception:
        pass


def tick_stage(area):
    preview = getattr(area, "_keybackcon_preview", None)
    if preview is not None:
        try:
            if not preview._effects_enabled or not animations_enabled():
                area.queue_draw()
                return GLib.SOURCE_CONTINUE
            # Escena estática (fijar sin partículas): repintar menos a menudo.
            if preview.mode == "fijar" and preview._palive == 0:
                area.queue_draw()
                return GLib.SOURCE_CONTINUE
        except Exception:
            pass
    area.queue_draw()
    return GLib.SOURCE_CONTINUE


class StagePreview:
    def __init__(self, color_hex="ffffff", pct=100, mode="fijar"):
        self.color_hex = color_hex
        self.pct = pct
        self.mode = mode
        self.t0 = GLib.get_monotonic_time()
        self._effects_enabled = True
        self._palive = 0
        self._spawn_acc = 0.0
        self._last_us = self.t0
        self._px = [0.0] * _MAX_PARTICLES
        self._py = [0.0] * _MAX_PARTICLES
        self._pvx = [0.0] * _MAX_PARTICLES
        self._pvy = [0.0] * _MAX_PARTICLES
        self._page = [0.0] * _MAX_PARTICLES
        self._plife = [1.0] * _MAX_PARTICLES
        self._psize = [2.0] * _MAX_PARTICLES
        self._pr = [1.0] * _MAX_PARTICLES
        self._pg = [1.0] * _MAX_PARTICLES
        self._pb = [1.0] * _MAX_PARTICLES

    def set_color(self, color_hex):
        self.color_hex = color_hex

    def set_brightness(self, pct):
        self.pct = pct

    def set_mode(self, mode):
        self.mode = mode
        self._palive = 0
        self._spawn_acc = 0.0

    def reset_clock(self):
        now = GLib.get_monotonic_time()
        self.t0 = now
        self._last_us = now
        self._palive = 0
        self._spawn_acc = 0.0

    def set_effects_enabled(self, enabled):
        self._effects_enabled = bool(enabled)
        if not self._effects_enabled:
            self._palive = 0
            self._spawn_acc = 0.0

    def draw(self, area, ctx, w, h, _data):
        draw_stage(area, ctx, w, h, color_hex=self.color_hex, pct=self.pct, mode=self.mode, t0=self.t0)
        now = GLib.get_monotonic_time()
        dt = (now - self._last_us) / 1000000.0
        if dt < 0.0:
            dt = 0.0
        elif dt > _DT_MAX:
            dt = _DT_MAX
        self._last_us = now
        try:
            ww = float(w)
            hh = float(h)
        except Exception:
            return
        if ww <= 0.0 or hh <= 0.0:
            return
        if not self._effects_enabled:
            self._palive = 0
            self._spawn_acc = 0.0
            return
        try:
            animate = animations_enabled()
        except Exception:
            animate = True
        if not animate:
            self._palive = 0
            self._spawn_acc = 0.0
            return
        t = (now - self.t0) / 1000000.0
        mode = self.mode
        if mode == "breathe":
            self._paint_breathe(ctx, ww, hh, t, dt)
            return
        if mode == "rainbow":
            self._paint_rainbow(ctx, ww, hh, t, dt)
            return
        self._paint_fijar(ctx, ww, hh, t, dt)

    def tick(self, area):
        return tick_stage(area)

    def attach(self, area, interval=33):
        self._last_us = GLib.get_monotonic_time()
        area._keybackcon_preview = self
        area.set_draw_func(self.draw, None)
        # Con movimiento reducido o escena fija, intervalo más largo (UX-19).
        try:
            if not self._effects_enabled or not animations_enabled():
                interval = max(interval, 250)
            elif self.mode == "fijar":
                interval = max(interval, 100)
        except Exception:
            pass
        return GLib.timeout_add(interval, self.tick, area)

    def _paint_fijar(self, ctx, w, h, t, dt):
        try:
            pct = float(self.pct)
        except Exception:
            pct = 100.0
        try:
            r, g, b = scale_rgb(self.color_hex, pct)
        except Exception:
            r, g, b = (1.0, 1.0, 1.0)
        pulse = 0.5 + 0.5 * math.sin(t * 2.0 * math.pi / _FIX_PULSE_PERIOD)
        cx = w / 2.0
        cy = h / 2.0 - 8.0
        grow = 1.0 + 0.035 * (pulse - 0.5) * 2.0
        ctx.set_source_rgba(r, g, b, 0.05 + 0.04 * pulse)
        ctx.arc(cx, cy, 80.0 * grow, 0, 2 * math.pi)
        ctx.fill()
        ctx.set_source_rgba(r, g, b, 0.10 * (0.85 + 0.30 * pulse))
        ctx.arc(cx, cy, 64.0 * grow, 0, 2 * math.pi)
        ctx.fill()
        alive = self._palive
        if alive > 0:
            px = self._px
            py = self._py
            pvx = self._pvx
            pvy = self._pvy
            page = self._page
            plife = self._plife
            psize = self._psize
            pr = self._pr
            pg = self._pg
            pb = self._pb
            i = alive - 1
            while i >= 0:
                age = page[i] + dt
                life = plife[i]
                if age >= life:
                    alive -= 1
                    if i != alive:
                        px[i] = px[alive]
                        py[i] = py[alive]
                        pvx[i] = pvx[alive]
                        pvy[i] = pvy[alive]
                        page[i] = page[alive]
                        plife[i] = plife[alive]
                        psize[i] = psize[alive]
                        pr[i] = pr[alive]
                        pg[i] = pg[alive]
                        pb[i] = pb[alive]
                    i -= 1
                    continue
                page[i] = age
                x = px[i] + pvx[i] * dt
                y = py[i] + pvy[i] * dt
                px[i] = x
                py[i] = y
                fade = 1.0 - age / life
                ctx.set_source_rgba(pr[i], pg[i], pb[i], fade * 0.45)
                ctx.arc(x, y, psize[i], 0, 2 * math.pi)
                ctx.fill()
                i -= 1
            self._palive = alive

    def _paint_breathe(self, ctx, w, h, t, dt):
        k = (1.0 - math.cos(t * 2.0 * math.pi / _BREATHE_PERIOD)) / 2.0
        try:
            pct = float(self.pct)
        except Exception:
            pct = 100.0
        if pct <= 0.0:
            rate = 0.0
            br = 0.0
            bg = 0.0
            bb = 0.0
        else:
            rate = _BREATHE_RATE_BASE + _BREATHE_RATE_SPAN * k
            try:
                br, bg, bb = hex_to_rgb(self.color_hex)
            except Exception:
                br, bg, bb = (1.0, 1.0, 1.0)
            dim = pct / 100.0
            br *= dim
            bg *= dim
            bb *= dim
        cx = w / 2.0
        cy = h / 2.0 - 8.0
        rise = 20.0 + 50.0 * k
        alive = self._palive
        acc = self._spawn_acc + rate * dt
        n_new = int(acc)
        acc -= n_new
        free = _MAX_PARTICLES - alive
        if n_new > free:
            n_new = free
            acc = 0.0
        if n_new < 0:
            n_new = 0
        px = self._px
        py = self._py
        pvx = self._pvx
        pvy = self._pvy
        page = self._page
        plife = self._plife
        psize = self._psize
        pr = self._pr
        pg = self._pg
        pb = self._pb
        rnd = random.random
        for _n in range(n_new):
            px[alive] = cx - 60.0 + rnd() * 120.0
            py[alive] = cy + rnd() * 12.0
            pvx[alive] = (rnd() - 0.5) * 16.0
            pvy[alive] = -(rise + rnd() * 22.0)
            page[alive] = 0.0
            plife[alive] = 1.8 + rnd() * 1.2
            psize[alive] = 1.5 + rnd() * 2.0
            pr[alive] = br
            pg[alive] = bg
            pb[alive] = bb
            alive += 1
        self._spawn_acc = acc
        i = alive - 1
        while i >= 0:
            age = page[i] + dt
            life = plife[i]
            if age >= life:
                alive -= 1
                if i != alive:
                    px[i] = px[alive]
                    py[i] = py[alive]
                    pvx[i] = pvx[alive]
                    pvy[i] = pvy[alive]
                    page[i] = page[alive]
                    plife[i] = plife[alive]
                    psize[i] = psize[alive]
                    pr[i] = pr[alive]
                    pg[i] = pg[alive]
                    pb[i] = pb[alive]
                i -= 1
                continue
            page[i] = age
            x = px[i] + pvx[i] * dt
            y = py[i] + pvy[i] * dt
            px[i] = x
            py[i] = y
            fade = 1.0 - age / life
            ctx.set_source_rgba(pr[i], pg[i], pb[i], fade * 0.55)
            ctx.arc(x, y, psize[i], 0, 2 * math.pi)
            ctx.fill()
            i -= 1
        self._palive = alive

    def _paint_rainbow(self, ctx, w, h, t, dt):
        try:
            pct = float(self.pct)
        except Exception:
            pct = 100.0
        if pct <= 0.0:
            rate = 0.0
            dim = 0.0
        else:
            rate = _RAINBOW_RATE
            dim = pct / 100.0
        cx = w / 2.0
        cy = h / 2.0 - 8.0
        base_hue = (t * _RAINBOW_DEG_PER_SEC) % 360.0
        alive = self._palive
        acc = self._spawn_acc + rate * dt
        n_new = int(acc)
        acc -= n_new
        free = _MAX_PARTICLES - alive
        if n_new > free:
            n_new = free
            acc = 0.0
        if n_new < 0:
            n_new = 0
        px = self._px
        py = self._py
        pvx = self._pvx
        pvy = self._pvy
        page = self._page
        plife = self._plife
        psize = self._psize
        pr = self._pr
        pg = self._pg
        pb = self._pb
        rnd = random.random
        for _n in range(n_new):
            ang = rnd() * 2.0 * math.pi
            speed = 18.0 + rnd() * 55.0
            px[alive] = cx + (rnd() - 0.5) * 120.0
            py[alive] = cy + (rnd() - 0.5) * 30.0
            pvx[alive] = math.cos(ang) * speed
            pvy[alive] = math.sin(ang) * speed * 0.7 - 8.0
            page[alive] = 0.0
            plife[alive] = 0.7 + rnd() * 0.6
            psize[alive] = 2.0 + rnd() * 2.0
            hue = (base_hue + rnd() * 36.0) % 360.0
            try:
                rr, gg, bb = hsv_to_rgb(hue, 1.0, 1.0)
            except Exception:
                rr, gg, bb = (1.0, 1.0, 1.0)
            pr[alive] = rr * dim
            pg[alive] = gg * dim
            pb[alive] = bb * dim
            alive += 1
        self._spawn_acc = acc
        i = alive - 1
        while i >= 0:
            age = page[i] + dt
            life = plife[i]
            if age >= life:
                alive -= 1
                if i != alive:
                    px[i] = px[alive]
                    py[i] = py[alive]
                    pvx[i] = pvx[alive]
                    pvy[i] = pvy[alive]
                    page[i] = page[alive]
                    plife[i] = plife[alive]
                    psize[i] = psize[alive]
                    pr[i] = pr[alive]
                    pg[i] = pg[alive]
                    pb[i] = pb[alive]
                i -= 1
                continue
            page[i] = age
            x = px[i] + pvx[i] * dt
            y = py[i] + pvy[i] * dt
            px[i] = x
            py[i] = y
            fade = 1.0 - age / life
            ctx.set_source_rgba(pr[i], pg[i], pb[i], fade * 0.75)
            ctx.arc(x, y, psize[i], 0, 2 * math.pi)
            ctx.fill()
            i -= 1
        self._palive = alive
