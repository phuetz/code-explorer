import { readFileSync } from 'node:fs';

const root = new URL('../', import.meta.url);
const read = (path) => readFileSync(new URL(path, root), 'utf8');
const licenseId = (path) => {
  const text = read(path);
  if (text.startsWith('Business Source License 1.1\n')) return 'BUSL-1.1';
  if (text.startsWith('MIT License\n')) return 'MIT';
  throw new Error(`Licence inconnue dans ${path}`);
};

const rootLicense = licenseId('LICENSE');
const manifests = [
  { path: 'crates/code-explorer-desktop/ui/package.json', expected: rootLicense, required: true },
  { path: 'nexus-brain/package.json', expected: rootLicense, required: true },
  { path: 'chat-ui/package.json', expected: licenseId('chat-ui/LICENSE'), required: false },
];

for (const { path, expected, required } of manifests) {
  const manifest = JSON.parse(read(path));
  if (required && !Object.hasOwn(manifest, 'license')) {
    throw new Error(`${path} : métadonnée license absente`);
  }
  if (Object.hasOwn(manifest, 'license') && manifest.license !== expected) {
    throw new Error(`${path} : ${manifest.license} contredit le fichier LICENSE (${expected})`);
  }
}

console.log('Métadonnées de licence cohérentes avec les fichiers LICENSE');
