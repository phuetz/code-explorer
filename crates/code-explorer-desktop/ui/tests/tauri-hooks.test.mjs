import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

// Tauri executes string hooks in the detected frontend directory.
test('the configured Tauri build hook builds the frontend from its directory', () => {
  const configPath = fileURLToPath(new URL('../../tauri.conf.json', import.meta.url));
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  const frontend = dirname(resolve(dirname(configPath), config.build.frontendDist));
  const result = spawnSync(config.build.beforeBuildCommand, {
    cwd: frontend,
    shell: true,
    encoding: 'utf8',
    timeout: 120000,
  });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
});

test('the bundle declares a square PNG icon usable by AppImage', () => {
  const configPath = fileURLToPath(new URL('../../tauri.conf.json', import.meta.url));
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  const squareIcons = config.bundle.icon.filter((path) => path.endsWith('.png')).filter((path) => {
    const png = readFileSync(resolve(dirname(configPath), path));
    assert.equal(png.subarray(1, 4).toString(), 'PNG');
    return png.readUInt32BE(16) > 0 && png.readUInt32BE(16) === png.readUInt32BE(20);
  });
  assert.ok(squareIcons.length > 0, 'AppImage requires a square PNG in bundle.icon');
});
