#!/usr/bin/env node
// 发版前的一致性守卫：tag 必须与三处应用版本完全一致。
//
// 为什么需要：tauri-action 以 tag 为准去建 Release。若 tag 是 v9.9.9 而
// tauri.conf.json 里还是 1.0.0，就会打出一个「自称 1.0.0 却挂在 v9.9.9 下」
// 的包，下载页面上完全看不出问题，直到有人报告版本对不上。
//
// 顺带校验三处版本彼此同步 —— 它们必须手工一起改，最容易漂移。
//
// 用法：
//   node scripts/check-release-version.mjs v1.0.1     # CI 与本地发版前都跑这个
//   node scripts/check-release-version.mjs           # 只校验三处彼此一致

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function readJson(rel) {
  return JSON.parse(readFileSync(join(root, rel), "utf8"));
}

/// 从 Cargo.toml 的 [package] 段取 version，避免引入 TOML 依赖。
function readCargoVersion(rel) {
  const src = readFileSync(join(root, rel), "utf8");
  const pkg = src.split(/^\s*\[/m).find((s) => s.startsWith("package]"));
  if (!pkg) throw new Error(`${rel}: 找不到 [package] 段`);
  const m = pkg.match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!m) throw new Error(`${rel}: [package] 段里没有 version`);
  return m[1];
}

const sources = {
  "src-tauri/tauri.conf.json": readJson("src-tauri/tauri.conf.json").version,
  "src-tauri/Cargo.toml": readCargoVersion("src-tauri/Cargo.toml"),
  "package.json": readJson("package.json").version,
};

const entries = Object.entries(sources);
const unique = [...new Set(entries.map(([, v]) => v))];

if (unique.length > 1) {
  console.error("::error::版本号在各文件中不一致：");
  for (const [file, v] of entries) console.error(`  ${file} = ${v}`);
  console.error("\n请把它们改成同一个版本号后再发版。");
  process.exit(1);
}

const appVersion = unique[0];
const tag = process.argv[2];

console.log(`应用版本：${appVersion}`);
if (!tag) {
  console.log("未提供 tag，仅校验各文件版本一致。");
  process.exit(0);
}

if (tag !== `v${appVersion}`) {
  console.error(`::error::tag ${tag} 与应用版本 v${appVersion} 不一致。`);
  console.error(
    tag.startsWith("v")
      ? `要么把 tag 改成 v${appVersion}，要么把三处版本号改成 ${tag.slice(1)}。`
      : `tag 需要以 v 开头，例如 v${appVersion}。`,
  );
  process.exit(1);
}

console.log(`tag ${tag} 与应用版本一致。`);