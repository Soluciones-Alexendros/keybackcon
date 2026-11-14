#!/usr/bin/env node
// Lanzador del binario precompilado de la plataforma (modelo esbuild):
// el paquete meta `ilutex` declara los binarios como optionalDependencies
// por arquitectura; aquí se resuelve el que esté instalado.
'use strict';

const { spawn } = require('child_process');

function resolveBinary() {
  for (const name of ['ilutex-linux-x64', 'ilutex-linux-arm64']) {
    try {
      const bin = require(name);
      if (typeof bin === 'string') return bin;
    } catch {
      // paquete de plataforma no instalado (arquitectura distinta)
    }
  }
  return null;
}

const bin = resolveBinary();
if (!bin) {
  console.error(
    'ilutex: no hay binario precompilado para esta plataforma.\n' +
      'Soportado: Linux x86_64 y arm64. Instala desde código con scripts/install.sh.'
  );
  process.exit(1);
}

const child = spawn(bin, process.argv.slice(2), { stdio: 'inherit' });
child.on('error', (err) => {
  console.error(`ilutex: no se pudo ejecutar ${bin}: ${err.message}`);
  process.exit(1);
});
child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code == null ? 1 : code);
  }
});