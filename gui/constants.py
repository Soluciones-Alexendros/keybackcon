"""Constantes compartidas de la GUI (evita duplicación con Rust y entre módulos Python)."""

import os

# Paso de brillo para teclas +/-, rueda y botones.
BRILLO_PASO = 5

# Animación breathe (duplicada en src/animation.rs por ADR 9).
BREATHE_PERIOD = 4.0
BREATHE_RATE_BASE = 8.0
BREATHE_RATE_SPAN = 30.0
FIX_PULSE_PERIOD = 3.0

# Animación rainbow (duplicada en src/animation.rs por ADR 9).
RAINBOW_DEG_PER_SEC = 45.0
RAINBOW_RATE = 42.0

# Límites de renderizado.
MAX_PARTICLES = 120
DT_MAX = 0.05

# UI
DEFAULT_WINDOW_WIDTH = 460
DEFAULT_WINDOW_HEIGHT = 840
MIN_WINDOW_WIDTH = 420
MIN_WINDOW_HEIGHT = 620
STAGE_HEIGHT = 150
SWATCH_SIZE = 40
COLORS_PER_ROW = 6

# Temporizadores
STATE_POLL_SECONDS = 2
BRIGHTNESS_DEBOUNCE_MS = 150
TRAY_TICK_SECONDS = 2

# Omarchy
OMARCHY_THEME_DIR = os.path.join(os.path.expanduser("~/.local/state/omarchy/current"))
OMARCHY_COLORS_TOML = os.path.join(OMARCHY_THEME_DIR, "theme", "colors.toml")
OMARCHY_ACCENT_FALLBACK = "#FFD60A"

# CSS
CSS_PRIORITY = "APPLICATION"