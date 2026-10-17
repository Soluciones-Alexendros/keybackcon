#!/usr/bin/env node
// Postinstalación de keybackcon: deja el paquete listo para funcionar.
//  1. Regla udev -> /etc/udev/rules.d (sudo/pkexec; opt-out con
//     KEYBACKCON_SKIP_UDEV=1).
//  2. Unidades systemd de usuario -> ~/.config/systemd/user, con la ruta del
//     binario resuelta, y recarga de systemd. No activa nada que el usuario
//     hubiera desactivado; solo habilita la restauración al login si la unidad
//     es nueva.
// Nada de esto aborta la instalación: sin udev/systemd el binario sigue
// disponible y avisa de lo que falta.
'use strict';

const { execFileSync, spawnSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const ROOT = __dirname;

function log(msg) {
  console.log(`keybackcon: ${msg}`);
}

function warn(msg) {
  console.warn(`keybackcon: aviso: ${msg}`);
}

function resolveBinary() {
  for (const name of ['keybackcon-linux-x64', 'keybackcon-linux-arm64']) {
    try {
      const bin = require(name);
      if (typeof bin === 'string' && fs.existsSync(bin)) return bin;
    } catch {
      // paquete de plataforma no instalado
    }
  }
  return null;
}

function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { stdio: 'inherit', ...opts });
  return r.status === 0;
}

function installUdev() {
  if (process.env.KEYBACKCON_SKIP_UDEV === '1') {
    log('regla udev omitida (KEYBACKCON_SKIP_UDEV=1)');
    return;
  }
  const dest = '/etc/udev/rules.d/70-keybackcon.rules';
  if (fs.existsSync(dest)) return;
  const src = path.join(ROOT, 'udev', '70-keybackcon.rules');
  if (process.getuid && process.getuid() === 0) {
    if (run('install', ['-m644', src, dest])) {
      run('udevadm', ['control', '--reload']);
      log('regla udev instalada (root)');
      return;
    }
  } else if (run('sudo', ['-n', 'install', '-m644', src, dest])) {
    run('sudo', ['-n', 'udevadm', 'control', '--reload']);
    log('regla udev instalada (sudo)');
    return;
  } else if (run('pkexec', ['install', '-m644', src, dest])) {
    run('pkexec', ['udevadm', 'control', '--reload']);
    log('regla udev instalada (pkexec)');
    return;
  }
  warn(
    `no se pudo instalar la regla udev (hace falta para acceder al teclado).\n` +
      `   Instálala a mano: sudo install -m644 ${src} ${dest} && sudo udevadm control --reload`
  );
}

function installWriterLock() {
  if (process.env.KEYBACKCON_SKIP_UDEV === '1') {
    log('candado de escritura omitido (KEYBACKCON_SKIP_UDEV=1)');
    return;
  }
  const sys = path.join(ROOT, 'sysusers.d', 'keybackcon.conf');
  const tmp = path.join(ROOT, 'tmpfiles.d', 'keybackcon.conf');
  const sysDest = '/usr/lib/sysusers.d/keybackcon.conf';
  const tmpDest = '/usr/lib/tmpfiles.d/keybackcon.conf';
  const cmds = process.getuid && process.getuid() === 0
    ? [
        ['install', ['-D', '-m644', sys, sysDest]],
        ['install', ['-D', '-m644', tmp, tmpDest]],
        ['systemd-sysusers', ['keybackcon.conf']],
        ['systemd-tmpfiles', ['--create', 'keybackcon.conf']],
      ]
    : null;
  if (cmds) {
    cmds.forEach(([cmd, args]) => run(cmd, args));
    log('candado de escritura instalado (root)');
    return;
  }
  const sudo = [
    ['install', ['-D', '-m644', sys, sysDest]],
    ['install', ['-D', '-m644', tmp, tmpDest]],
    ['systemd-sysusers', ['keybackcon.conf']],
    ['systemd-tmpfiles', ['--create', 'keybackcon.conf']],
  ];
  if (sudo.every(([cmd, args]) => run('sudo', ['-n', cmd, ...args]))) {
    log('candado de escritura instalado (sudo)');
    return;
  }
  warn(
    'no se pudo crear /run/keybackcon/writer.lock. Sin ese candado el CLI no escribe.\n' +
      `   sudo install -D -m644 ${sys} ${sysDest}\n` +
      `   sudo install -D -m644 ${tmp} ${tmpDest}\n` +
      '   sudo systemd-sysusers keybackcon.conf && sudo systemd-tmpfiles --create keybackcon.conf'
  );
}

function installSystemdUnits(bin) {
  if (spawnSync('systemctl', ['--user', 'show-environment'], { stdio: 'ignore' }).status !== 0) {
    warn('systemd de usuario no disponible; se omite la instalación de servicios');
    return;
  }
  const dir = path.join(os.homedir(), '.config', 'systemd', 'user');
  fs.mkdirSync(dir, { recursive: true });
  const units = ['keybackcon.service', 'keybackcon-animation@.service'];
  let anyNew = false;
  for (const unit of units) {
    const tmpl = fs.readFileSync(path.join(ROOT, 'systemd', unit), 'utf8');
    const rendered = tmpl.split('__KEYBACKCON_BIN__').join(bin);
    const dest = path.join(dir, unit);
    const existing = fs.existsSync(dest) ? fs.readFileSync(dest, 'utf8') : null;
    if (existing !== rendered) {
      fs.writeFileSync(dest, rendered);
      anyNew = true;
      log(`unidad ${unit} instalada en ~/.config/systemd/user`);
    }
  }
  run('systemctl', ['--user', 'daemon-reload']);
  const enable = path.join(dir, 'keybackcon.service');
  if (anyNew && fs.existsSync(enable)) {
    // Solo si la unidad es nueva (no existe estado previo del usuario).
    run('systemctl', ['--user', 'enable', 'keybackcon.service']);
    log('restauración al login habilitada (systemctl --user enable keybackcon.service)');
  }
}

function main() {
  const bin = resolveBinary();
  if (!bin) {
    warn(
      'no se encontró el binario de plataforma; solo se instala lo que sea posible'
    );
  } else {
    log(`binario: ${bin}`);
  }
  installUdev();
  installWriterLock();
  if (bin) installSystemdUnits(bin);
  log('listo. Prueba con: keybackcon info');
}

main();
