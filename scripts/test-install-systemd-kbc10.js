#!/usr/bin/env node
// KBC-10: installSystemdUnits respeta disabled|masked y habilita en alta nueva.
// HOME temporal y systemctl falso al inicio de PATH. Sin sudo ni systemctl real.
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');

const ROOT = path.join(__dirname, '..');
const INSTALL_JS = path.join(ROOT, 'npm', 'install.js');

const prevHome = process.env.HOME;
const prevPath = process.env.PATH;
const prevSkipUdev = process.env.KEYBACKCON_SKIP_UDEV;
const prevForceEnable = process.env.KEYBACKCON_ENABLE_RESTORE;

function restoreEnv() {
  if (prevHome === undefined) delete process.env.HOME;
  else process.env.HOME = prevHome;
  if (prevPath === undefined) delete process.env.PATH;
  else process.env.PATH = prevPath;
  if (prevSkipUdev === undefined) delete process.env.KEYBACKCON_SKIP_UDEV;
  else process.env.KEYBACKCON_SKIP_UDEV = prevSkipUdev;
  if (prevForceEnable === undefined) delete process.env.KEYBACKCON_ENABLE_RESTORE;
  else process.env.KEYBACKCON_ENABLE_RESTORE = prevForceEnable;
  try {
    delete require.cache[require.resolve(INSTALL_JS)];
  } catch {
    // ignore
  }
}

function writeFakeSystemctl(binDir, logPath, isEnabledStdout, isEnabledExit) {
  const fakeSystemctl = path.join(binDir, 'systemctl');
  fs.writeFileSync(
    fakeSystemctl,
    `#!/bin/sh
echo "$*" >> "${logPath}"
case "$*" in
  "--user show-environment") exit 0 ;;
  "--user daemon-reload") exit 0 ;;
  "--user enable "*) exit 0 ;;
  "--user is-enabled "*)
    echo ${isEnabledStdout}
    exit ${isEnabledExit}
    ;;
  *) exit 0 ;;
esac
`,
    { mode: 0o755 }
  );
}

function runCase(name, opts) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'keybackcon-kbc10-'));
  const home = path.join(tmp, 'home');
  const binDir = path.join(tmp, 'bin');
  const logPath = path.join(tmp, 'systemctl.log');
  const fakeBin = path.join(tmp, 'fake-keybackcon');
  const unitDir = path.join(home, '.config', 'systemd', 'user');
  const unit = path.join(unitDir, 'keybackcon.service');
  const oldUnit =
    '[Unit]\nDescription=old\n[Service]\nExecStart=/old/path restore\n[Install]\nWantedBy=default.target\n';

  try {
    fs.mkdirSync(home, { recursive: true });
    fs.mkdirSync(binDir, { recursive: true });
    fs.mkdirSync(unitDir, { recursive: true });
    fs.writeFileSync(fakeBin, '#!/bin/sh\nexit 0\n', { mode: 0o755 });

    if (opts.preExistingUnit) {
      fs.writeFileSync(unit, oldUnit);
    }

    writeFakeSystemctl(
      binDir,
      logPath,
      opts.isEnabledStdout,
      opts.isEnabledExit
    );

    process.env.HOME = home;
    process.env.PATH = `${binDir}:${prevPath || ''}`;
    process.env.KEYBACKCON_SKIP_UDEV = '1';
    delete process.env.KEYBACKCON_ENABLE_RESTORE;

    delete require.cache[require.resolve(INSTALL_JS)];
    const { installSystemdUnits } = require(INSTALL_JS);
    installSystemdUnits(fakeBin);

    const log = fs.existsSync(logPath) ? fs.readFileSync(logPath, 'utf8') : '';
    const enabled = /--user enable\b/.test(log);
    const newUnit = fs.existsSync(unit) ? fs.readFileSync(unit, 'utf8') : '';
    const contentOk = newUnit.includes(fakeBin);
    const wroteOutsideTmp = newUnit.includes('/home/alexendros');
    const queriedIsEnabled = /--user is-enabled\b/.test(log);

    console.log(`\n[${name}]`);
    console.log(`  HOME temporal: ${home}`);
    console.log(`  estado modelado: ${opts.isEnabledStdout}`);
    console.log(`  unidad previa: ${opts.preExistingUnit}`);
    console.log(`  consultó is-enabled: ${queriedIsEnabled}`);
    console.log(`  systemctl log:\n${log.trim() || '(vacío)'}`);
    console.log(`  recibió enable: ${enabled}`);

    if (wroteOutsideTmp) {
      throw new Error(`${name}: se escribió fuera del HOME temporal`);
    }
    if (!contentOk) {
      throw new Error(`${name}: la unidad no se escribió con el binario falso`);
    }
    if (!queriedIsEnabled) {
      throw new Error(`${name}: no se consultó systemctl --user is-enabled`);
    }
    if (opts.expectEnable && !enabled) {
      throw new Error(`${name}: se esperaba enable y no se observó`);
    }
    if (!opts.expectEnable && enabled) {
      throw new Error(`${name}: no debía recibir enable y lo recibió`);
    }

    console.log(`  OK`);
    return true;
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
    restoreEnv();
  }
}

let failed = 0;

try {
  console.log('KBC-10: install.js + systemctl falso (disabled / masked / alta nueva)');

  // Negativo: unidad existente + disabled + contenido distinto → NO enable.
  if (
    !runCase('disabled + unidad existente', {
      preExistingUnit: true,
      isEnabledStdout: 'disabled',
      isEnabledExit: 1,
      expectEnable: false,
    })
  ) {
    failed += 1;
  }

  // Negativo: masked → NO enable.
  if (
    !runCase('masked + unidad existente', {
      preExistingUnit: true,
      isEnabledStdout: 'masked',
      isEnabledExit: 1,
      expectEnable: false,
    })
  ) {
    failed += 1;
  }

  // Positivo: primera instalación (sin unidad) → enable permitido.
  if (
    !runCase('alta nueva sin unidad', {
      preExistingUnit: false,
      isEnabledStdout: 'not-found',
      isEnabledExit: 1,
      expectEnable: true,
    })
  ) {
    failed += 1;
  }

  if (failed) {
    console.error(`\nKBC-10: ${failed} caso(s) fallaron`);
    process.exit(1);
  }
  console.log('\nKBC-10: todos los casos pasaron (aceptación alineada con install.sh).');
  process.exit(0);
} catch (err) {
  console.error(err);
  restoreEnv();
  process.exit(1);
}
