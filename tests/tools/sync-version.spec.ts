// 版本同步脚本单测（对应 tools/sync-version.mjs）。
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "../../tools/sync-version.mjs");

let root = "";

function writeFixture(version = "0.0.1") {
  writeFileSync(
    join(root, "package.json"),
    `${JSON.stringify({ name: "chatters", version }, null, 2)}\n`,
  );
  writeFileSync(
    join(root, "package-lock.json"),
    `${JSON.stringify(
      {
        name: "chatters",
        version,
        lockfileVersion: 3,
        packages: { "": { name: "chatters", version } },
      },
      null,
      2,
    )}\n`,
  );
  mkdirSync(join(root, "src-tauri"), { recursive: true });
  writeFileSync(
    join(root, "src-tauri/tauri.conf.json"),
    `${JSON.stringify({ productName: "Chatters", version }, null, 2)}\n`,
  );
  writeFileSync(
    join(root, "src-tauri/Cargo.toml"),
    `[package]\nname = "chatters"\nversion = "${version}"\n\n[dependencies]\nserde = { version = "1", features = ["derive"] }\n`,
  );
  writeFileSync(
    join(root, "src-tauri/Cargo.lock"),
    `[[package]]\nname = "serde"\nversion = "1.0.0"\n\n[[package]]\nname = "chatters"\nversion = "${version}"\ndependencies = []\n`,
  );
}

function run(tag: string) {
  execFileSync(process.execPath, [SCRIPT, tag], { cwd: root, stdio: "pipe" });
}

function readJson(rel: string): {
  version?: string;
  packages?: Record<string, { version?: string }>;
} {
  return JSON.parse(readFileSync(join(root, rel), "utf8")) as {
    version?: string;
    packages?: Record<string, { version?: string }>;
  };
}

describe("sync-version", () => {
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), "chatters-sync-"));
    writeFixture();
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  it("把 tag 写入 package / tauri / cargo 五处版本字段", () => {
    run("v1.2.3-rc.4");

    expect(readJson("package.json").version).toBe("1.2.3-rc.4");
    const lock = readJson("package-lock.json");
    expect(lock.version).toBe("1.2.3-rc.4");
    expect(lock.packages?.[""]?.version).toBe("1.2.3-rc.4");
    expect(readJson("src-tauri/tauri.conf.json").version).toBe("1.2.3-rc.4");

    const cargoToml = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
    expect(cargoToml).toContain('version = "1.2.3-rc.4"');
    // 依赖版本不能被误改。
    expect(cargoToml).toContain('serde = { version = "1"');

    const cargoLock = readFileSync(join(root, "src-tauri/Cargo.lock"), "utf8");
    expect(cargoLock).toContain('name = "chatters"\nversion = "1.2.3-rc.4"');
    expect(cargoLock).toContain('name = "serde"\nversion = "1.0.0"');
  });

  it("重复执行幂等（第二次不改文件）", () => {
    run("1.2.3");
    const snapshot = [
      "package.json",
      "package-lock.json",
      "src-tauri/tauri.conf.json",
      "src-tauri/Cargo.toml",
      "src-tauri/Cargo.lock",
    ].map((rel) => readFileSync(join(root, rel), "utf8"));

    run("v1.2.3");
    const again = [
      "package.json",
      "package-lock.json",
      "src-tauri/tauri.conf.json",
      "src-tauri/Cargo.toml",
      "src-tauri/Cargo.lock",
    ].map((rel) => readFileSync(join(root, rel), "utf8"));

    expect(again).toEqual(snapshot);
  });

  it("非法 tag 退出码非 0", () => {
    expect(() => run("1.2")).toThrow();
  });
});
