import { readdir, stat } from "node:fs/promises";
import path from "node:path";

const distDir = path.resolve("dist", "assets");

function formatKb(bytes) {
  return `${(bytes / 1024).toFixed(2)} kB`;
}

async function main() {
  const entries = await readdir(distDir, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    if (!entry.isFile()) continue;
    const fullPath = path.join(distDir, entry.name);
    const info = await stat(fullPath);
    files.push({ name: entry.name, bytes: info.size });
  }

  files.sort((a, b) => b.bytes - a.bytes);

  const total = files.reduce((sum, file) => sum + file.bytes, 0);
  const large = files.filter((file) => file.bytes >= 100 * 1024);

  console.log("Bundle report");
  console.log("=============");
  console.log(`Assets: ${files.length}`);
  console.log(`Total size: ${formatKb(total)}`);
  console.log(`>=100 kB assets: ${large.length}`);
  console.log("");

  for (const file of files.slice(0, 30)) {
    const marker = file.bytes >= 500 * 1024 ? "!!" : file.bytes >= 100 * 1024 ? "!" : " ";
    console.log(`${marker} ${file.name.padEnd(48)} ${formatKb(file.bytes).padStart(10)}`);
  }
}

main().catch((error) => {
  console.error("Failed to generate bundle report:", error);
  process.exit(1);
});
