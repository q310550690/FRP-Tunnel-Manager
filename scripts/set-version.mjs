// 把发布 tag（v1.2.3）同步到三处版本号，CI 打包前调用：
//   node scripts/set-version.mjs v1.2.3
// 单一来源仍是 src-tauri/tauri.conf.json，这里只是让其它清单跟随 tag。
// 只做「逐字替换」，不重新序列化 JSON，避免无关格式变动。
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const raw = (process.argv[2] ?? "").trim();
const version = raw.replace(/^v/i, "");

// 允许 1.2.3 与 1.2.3-rc.1；其它形式直接失败，避免把奇怪 tag 写进清单
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`[set-version] 版本号不合法：${JSON.stringify(raw)}（期望 v1.2.3）`);
  process.exit(1);
}

/** 替换文件里第一处 `"version": "..."`；ok=false 说明没找到 */
function patchFirstVersion(rel, label) {
  const file = resolve(root, rel);
  const text = readFileSync(file, "utf8");
  if (!/"version"\s*:\s*"[^"]*"/.test(text)) {
    console.error(`[set-version] ${rel} 里没有顶层 "version" 字段`);
    process.exit(1);
  }
  const next = text.replace(/("version"\s*:\s*")[^"]*(")/, `$1${version}$2`);
  writeFileSync(file, next);
  console.log(`[set-version] ${label}：${version}`);
}

patchFirstVersion("package.json", "package.json");
patchFirstVersion("src-tauri/tauri.conf.json", "tauri.conf.json");

// Cargo.toml 只改 [package] 段里的第一处 version = "..."
const cargoFile = resolve(root, "src-tauri/Cargo.toml");
const cargo = readFileSync(cargoFile, "utf8");
const cargoVersionRe = /(\[package\][\s\S]*?\nversion = ")[^"]+(")/;
if (!cargoVersionRe.test(cargo)) {
  console.error("[set-version] 没在 src-tauri/Cargo.toml 的 [package] 段里找到 version");
  process.exit(1);
}
const patched = cargo.replace(cargoVersionRe, `$1${version}$2`);
writeFileSync(cargoFile, patched);
console.log(`[set-version] Cargo.toml：${version}`);

console.log(`[set-version] 完成：${version}`);
