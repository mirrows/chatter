#!/usr/bin/env node
// 把 Release tag 的版本号同步到代码中的 5 处版本字段（rules.md R7.2 / README 发版流程）。
//
// 用法：node tools/sync-version.mjs v1.0.0-rc.2
// 同步：package.json、package-lock.json、src-tauri/tauri.conf.json、
//       src-tauri/Cargo.toml、src-tauri/Cargo.lock
// 说明：只改版本字段，保持原文件格式；可重复执行（已同步时无改动）。

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

/** 校验并规范化 tag：接受 `v1.2.3` / `1.2.3-rc.1`。 */
export function normalizeVersion(tag) {
  const version = String(tag ?? "")
    .trim()
    .replace(/^v/i, "");
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`无效的版本标签：${tag}（期望 v1.2.3 或 v1.2.3-rc.1）`);
  }
  return version;
}

/**
 * 同步版本号。
 *
 * @param {string} tag Release tag（如 `v1.0.0-rc.2`）
 * @param {string} [root] 仓库根目录（测试时可指定临时目录）
 * @returns {{version: string, changed: string[]}}
 */
export function syncVersion(tag, root = process.cwd()) {
  const version = normalizeVersion(tag);
  const changed = [];

  // package.json / tauri.conf.json 各有 1 处 version；package-lock 有 2 处（根 + packages[""]）。
  const replaceJsonVersions = (rel, count) => {
    const file = join(root, rel);
    const before = readFileSync(file, "utf8");
    let seen = 0;
    const after = before.replace(/"version":\s*"[^"]*"/g, (match) => {
      seen += 1;
      return seen <= count ? `"version": "${version}"` : match;
    });
    if (seen < count) {
      throw new Error(`${rel} 中只找到 ${seen} 个 version 字段（期望 ${count} 个）`);
    }
    const parsed = JSON.parse(after);
    if (after !== before) {
      writeFileSync(file, after);
      changed.push(rel);
    }
    return parsed;
  };

  replaceJsonVersions("package.json", 1);
  const lock = replaceJsonVersions("package-lock.json", 2);
  replaceJsonVersions("src-tauri/tauri.conf.json", 1);

  if (lock.version !== version || lock.packages?.[""]?.version !== version) {
    throw new Error("package-lock.json 的根版本未同步成功");
  }

  const replaceText = (rel, pattern, replacement) => {
    const file = join(root, rel);
    const before = readFileSync(file, "utf8");
    if (!pattern.test(before)) {
      throw new Error(`${rel} 未找到版本字段 ${pattern}`);
    }
    const after = before.replace(pattern, replacement);
    if (after !== before) {
      writeFileSync(file, after);
      changed.push(rel);
    }
  };

  replaceText("src-tauri/Cargo.toml", /^version = "[^"]*"$/m, `version = "${version}"`);
  replaceText(
    "src-tauri/Cargo.lock",
    /(\[\[package\]\]\nname = "chatters"\nversion = ")[^"]*(")/,
    `$1${version}$2`,
  );

  return { version, changed };
}

const invokedDirectly = process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url;

if (invokedDirectly) {
  const tag = process.argv[2];
  if (!tag) {
    console.error("用法：node tools/sync-version.mjs <tag，如 v1.0.0-rc.2>");
    process.exit(2);
  }
  try {
    const { version, changed } = syncVersion(tag);
    console.log(`version=${version}`);
    console.log(changed.length ? `updated: ${changed.join(", ")}` : "no changes");
  } catch (error) {
    console.error(`ERROR: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}
