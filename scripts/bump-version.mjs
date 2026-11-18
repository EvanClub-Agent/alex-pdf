#!/usr/bin/env node
/**
 * 一键同步桌面工具的三处版本号:
 *   - package.json
 *   - src-tauri/Cargo.toml          ([package] section 里的 version)
 *   - src-tauri/tauri.conf.json
 *
 * 用法:
 *   node scripts/bump-version.mjs <new-version>
 *   node scripts/bump-version.mjs 1.0.1
 *   node scripts/bump-version.mjs patch    # 自动 +1 修订号 (1.0.0 -> 1.0.1)
 *   node scripts/bump-version.mjs minor    # 1.0.0 -> 1.1.0
 *   node scripts/bump-version.mjs major    # 1.0.0 -> 2.0.0
 *
 * 也支持作为 npm script 调用: `npm run bump <version>`
 */

import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const PACKAGE_JSON = resolve(ROOT, "package.json");
const CARGO_TOML = resolve(ROOT, "src-tauri/Cargo.toml");
const TAURI_CONF = resolve(ROOT, "src-tauri/tauri.conf.json");

const SEMVER_RE = /^\d+\.\d+\.\d+(?:[-+][\w.-]+)?$/;

function parseSemver(v) {
  const [core] = v.split(/[-+]/);
  const [major, minor, patch] = core.split(".").map(Number);
  return { major, minor, patch };
}

function bump(current, kind) {
  const { major, minor, patch } = parseSemver(current);
  if (kind === "major") return `${major + 1}.0.0`;
  if (kind === "minor") return `${major}.${minor + 1}.0`;
  if (kind === "patch") return `${major}.${minor}.${patch + 1}`;
  return null;
}

function updatePackageJson(version) {
  const raw = readFileSync(PACKAGE_JSON, "utf-8");
  const pkg = JSON.parse(raw);
  const old = pkg.version;
  pkg.version = version;
  // 保持 2 空格缩进 + 末尾换行，避免 git diff 噪音
  writeFileSync(PACKAGE_JSON, JSON.stringify(pkg, null, 2) + "\n");
  return old;
}

function updateLockfiles(current, target, packageName) {
  const npmPath = resolve(ROOT, "package-lock.json");
  const npmLock = JSON.parse(readFileSync(npmPath, "utf-8"));
  const cargoPath = resolve(ROOT, "src-tauri/Cargo.lock");
  const cargoLock = readFileSync(cargoPath, "utf-8");
  const cargoEntry = (version) => `[[package]]\nname = "${packageName}"\nversion = "${version}"\n`;
  if (npmLock.name !== packageName || npmLock.packages[""].name !== packageName
      || npmLock.version !== current || npmLock.packages[""].version !== current
      || !cargoLock.includes(cargoEntry(current))) {
    throw new Error("锁文件与项目名称或版本不一致，请先同步 npm 和 Cargo 的锁文件");
  }
  npmLock.version = target;
  npmLock.packages[""].version = target;
  writeFileSync(npmPath, JSON.stringify(npmLock, null, 2) + "\n");
  writeFileSync(cargoPath, cargoLock.replace(cargoEntry(current), cargoEntry(target)));
}

function updateCargoToml(version) {
  const raw = readFileSync(CARGO_TOML, "utf-8");
  // 只替换 [package] 段下的 version，避免误改 dependencies 里的版本
  const lines = raw.split("\n");
  let inPackage = false;
  let oldVersion = null;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();
    if (trimmed.startsWith("[")) {
      inPackage = trimmed === "[package]";
      continue;
    }
    if (!inPackage) continue;
    const match = line.match(/^(\s*version\s*=\s*)"([^"]+)"\s*$/);
    if (match) {
      oldVersion = match[2];
      lines[i] = `${match[1]}"${version}"`;
      break;
    }
  }
  if (oldVersion === null) {
    throw new Error(`Cargo.toml 中找不到 [package].version`);
  }
  writeFileSync(CARGO_TOML, lines.join("\n"));
  return oldVersion;
}

function updateTauriConf(version) {
  const raw = readFileSync(TAURI_CONF, "utf-8");
  const conf = JSON.parse(raw);
  const old = conf.version;
  conf.version = version;
  writeFileSync(TAURI_CONF, JSON.stringify(conf, null, 2) + "\n");
  return old;
}

function main() {
  const arg = process.argv[2];
  if (!arg) {
    console.error("用法: node scripts/bump-version.mjs <version|patch|minor|major>");
    process.exit(1);
  }

  // 先读当前 package.json 版本作为参考
  const pkg = JSON.parse(readFileSync(PACKAGE_JSON, "utf-8"));
  const currentPkg = pkg.version;
  let target = arg;
  if (["patch", "minor", "major"].includes(arg)) {
    target = bump(currentPkg, arg);
    if (!target) {
      console.error(`无法基于当前版本 ${currentPkg} 计算 ${arg} 升级`);
      process.exit(1);
    }
  } else if (!SEMVER_RE.test(arg)) {
    console.error(`非法版本号: ${arg}（要求 x.y.z 形式）`);
    process.exit(1);
  }

  updateLockfiles(currentPkg, target, pkg.name);
  const oldPkg = updatePackageJson(target);
  const oldCargo = updateCargoToml(target);
  const oldTauri = updateTauriConf(target);

  console.log(`✓ package.json          ${oldPkg} → ${target}`);
  console.log(`✓ src-tauri/Cargo.toml  ${oldCargo} → ${target}`);
  console.log(`✓ src-tauri/tauri.conf.json  ${oldTauri} → ${target}`);
  console.log("\n下一步:");
  console.log(`  1. npm run tauri build`);
  console.log(`  2. 把生成的安装包上传到更新服务器（http://localhost:8800/admin）`);
  console.log(`     版本号填: ${target}`);
}

main();
