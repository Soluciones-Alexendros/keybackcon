#!/usr/bin/env python3
"""Verifica la coherencia de los catálogos gettext (sin dependencias).

Comprueba que cada `po/*.po` cubra exactamente los msgid de
`po/keybackcon.pot`, que ninguno quede sin traducir ni marcado `fuzzy`, que no
haya msgid duplicados y que el idioma esté declarado en `gui/i18n.py`.

Uso: `python3 scripts/check-i18n.py [--extract]`

Con `--extract` extrae los msgid del código a un fichero temporal y verifica
que el `.pot` versionado coincida (detecta la deriva fuente → `.pot` sin
tocar el árbol); requiere gettext. Sin él, compara solo `.pot` → `.po`.
"""

import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
POT = os.path.join(ROOT, "po", "keybackcon.pot")
PO_DIR = os.path.join(ROOT, "po")
I18N_PY = os.path.join(ROOT, "gui", "i18n.py")

XGETTEXT_FLAGS = [
    "--language=Python",
    "--keyword=_",
    "--keyword=N_",
    "--from-code=UTF-8",
    "--add-comments=TRANSLATORS",
]


def unescape(raw):
    return raw.replace('\\"', '"').replace("\\n", "\n").replace("\\\\", "\\")


def parse(path):
    """Devuelve [(msgid, msgstr, flags)] de un .po/.pot."""
    entries = []
    msgid = None
    msgstr = None
    flags = []
    pending_flags = []

    def flush():
        nonlocal msgid, msgstr, flags
        if msgid is not None and msgid != "":
            entries.append((msgid, msgstr or "", flags))
        msgid = None
        msgstr = None
        flags = []

    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("#,"):
            pending_flags = [f.strip() for f in line[2:].split(",")]
            continue
        if line.startswith("#") or not line.strip():
            if not line.strip():
                flush()
                pending_flags = []
            continue
        if line.startswith("msgid_plural") or line.startswith("msgstr["):
            continue
        if line.startswith("msgid "):
            if msgid is not None:
                flush()
            msgid = unescape(line[6:].strip().strip('"'))
            flags = pending_flags
            pending_flags = []
            msgstr = None
            continue
        if line.startswith("msgstr "):
            msgstr = unescape(line[7:].strip().strip('"'))
            continue
        if line.startswith('"') and msgid is not None:
            value = unescape(line.strip().strip('"'))
            if msgstr is None:
                msgid += value
            else:
                msgstr += value
    flush()
    return entries


def supported_locales():
    src = open(I18N_PY, encoding="utf-8").read()
    match = re.search(r"SUPPORTED_LOCALES\s*=\s*\{([^}]*)\}", src)
    if not match:
        return set()
    return set(re.findall(r'"([a-zA-Z_]+)"', match.group(1)))


def extract_pot(dest):
    """Extrae los msgid del código a `dest`. True si xgettext funcionó."""
    cmd = ["xgettext"] + XGETTEXT_FLAGS + [
        "--package-name=keybackcon",
        "--output=" + dest,
    ]
    sources = sorted(
        os.path.join(ROOT, "gui", name)
        for name in os.listdir(os.path.join(ROOT, "gui"))
        if name.endswith(".py")
    )
    cmd += sources
    try:
        subprocess.run(cmd, check=True, cwd=ROOT)
    except (OSError, subprocess.CalledProcessError) as exc:
        print(f"check-i18n: no se pudo extraer los msgid: {exc}", file=sys.stderr)
        return False
    return True


def main(argv):
    pot_entries = parse(POT)
    pot_ids = [mid for mid, _msgstr, _flags in pot_entries]
    locales = supported_locales()
    fallos = []

    if "--extract" in argv:
        with tempfile.TemporaryDirectory() as tmp:
            fresh = os.path.join(tmp, "keybackcon.pot")
            if not extract_pot(fresh):
                return 1
            fresh_ids = [mid for mid, _m, _f in parse(fresh)]
        faltan = sorted(set(fresh_ids) - set(pot_ids))
        sobran = sorted(set(pot_ids) - set(fresh_ids))
        if faltan or sobran:
            fallos.append(
                "po/keybackcon.pot no está sincronizado con el código "
                f"(faltan {len(faltan)}: {faltan[:3]}; "
                f"sobran {len(sobran)}: {sobran[:3]}); ejecuta `make i18n`"
            )
        pot_ids = fresh_ids

    duplicados = {mid for mid in pot_ids if pot_ids.count(mid) > 1}
    if duplicados:
        fallos.append(f"{os.path.relpath(POT, ROOT)}: msgid duplicados {sorted(duplicados)}")

    if not locales:
        fallos.append("gui/i18n.py: no se pudo leer SUPPORTED_LOCALES")

    po_files = sorted(name for name in os.listdir(PO_DIR) if name.endswith(".po"))
    if not po_files:
        fallos.append("po/: no hay catálogos .po")
    for name in po_files:
        lang = name[:-3]
        if lang not in locales:
            fallos.append(f"po/{name}: idioma '{lang}' fuera de SUPPORTED_LOCALES")
        entries = parse(os.path.join(PO_DIR, name))
        ids = [mid for mid, _m, _f in entries]
        faltan = sorted(set(pot_ids) - set(ids))
        sobran = sorted(set(ids) - set(pot_ids))
        if faltan:
            fallos.append(f"po/{name}: faltan {len(faltan)} msgid del .pot: {faltan[:3]}")
        if sobran:
            fallos.append(f"po/{name}: sobran {len(sobran)} msgid: {sobran[:3]}")
        vacios = sorted(mid for mid, msgstr, _f in entries if not msgstr.strip())
        if vacios:
            fallos.append(f"po/{name}: sin traducir {len(vacios)}: {vacios[:3]}")
        difusos = sorted(mid for mid, _m, flags in entries if "fuzzy" in flags)
        if difusos:
            fallos.append(f"po/{name}: fuzzy {len(difusos)}: {difusos[:3]}")
        print(f"  ok: po/{name} ({len(ids)} msgid, idioma {lang})")
        for bad in (i for i in ids if i.count("{") != i.count("}")):
            fallos.append(f"po/{name}: llaves descompensadas en {bad!r}")

    print(f"  ok: {os.path.relpath(POT, ROOT)} ({len(pot_ids)} msgid)")
    if fallos:
        for fallo in fallos:
            print(f"  FALLO: {fallo}", file=sys.stderr)
        return 1
    print(f"check-i18n: OK ({len(pot_ids)} msgid x {len(po_files)} idiomas)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
