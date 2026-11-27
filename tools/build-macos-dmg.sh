#!/usr/bin/env bash
# 构建 macOS .app + .dmg（不依赖 Finder / AppleScript，CI 可用）。
#
# 用法：
#   bash tools/build-macos-dmg.sh
#   bash tools/build-macos-dmg.sh --target universal-apple-darwin --out /tmp/chatters-dist
#
# 说明：
# - 默认按本机架构构建（Apple Silicon → aarch64-apple-darwin）；
# - universal 需要先 rustup target add x86_64-apple-darwin；
# - DMG 用 hdiutil 生成（Tauri 自带 dmg 打包依赖 Finder，headless/CI 容易超时）。

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

case "$(uname -m)" in
    arm64) DEFAULT_TARGET="aarch64-apple-darwin" ;;
    x86_64) DEFAULT_TARGET="x86_64-apple-darwin" ;;
    *) DEFAULT_TARGET="" ;;
esac

TARGET="$DEFAULT_TARGET"
OUT_DIR="src-tauri/target/macos"

while [ $# -gt 0 ]; do
    case "$1" in
        --target) TARGET="${2:?缺少 --target 值}"; shift 2 ;;
        --out) OUT_DIR="${2:?缺少 --out 值}"; shift 2 ;;
        -h | --help)
            sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            printf 'ERROR: 未知参数 %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

[ "$(uname -s)" = "Darwin" ] || {
    echo "ERROR: 该脚本只能在 macOS 上运行" >&2
    exit 1
}
[ -n "$TARGET" ] || {
    echo "ERROR: 无法识别本机架构，请用 --target 指定" >&2
    exit 2
}

export PATH="$HOME/.cargo/bin:$PATH"

case "$TARGET" in
    universal-apple-darwin)
        command -v rustup >/dev/null 2>&1 || {
            echo "ERROR: universal 构建需要 rustup" >&2
            exit 1
        }
        for t in aarch64-apple-darwin x86_64-apple-darwin; do
            rustup target list --installed 2>/dev/null | grep -qx "$t" || {
                echo "ERROR: 缺少 Rust target ${t}（执行 rustup target add ${t}）" >&2
                exit 1
            }
        done
        BUNDLE="src-tauri/target/universal-apple-darwin/release/bundle"
        ARCH_TAG="universal"
        ;;
    aarch64-apple-darwin)
        BUNDLE="src-tauri/target/aarch64-apple-darwin/release/bundle"
        ARCH_TAG="arm64"
        ;;
    x86_64-apple-darwin)
        BUNDLE="src-tauri/target/x86_64-apple-darwin/release/bundle"
        ARCH_TAG="x64"
        ;;
    *)
        printf 'ERROR: 不支持的 target：%s\n' "$TARGET" >&2
        exit 2
        ;;
esac

echo "==> 构建 Chatters.app（target=${TARGET}）"
# 主配置开启 createUpdaterArtifacts（需要签名密钥）；macOS 暂不产出更新产物，
# 统一用 tauri.ci.conf.json 关闭，避免未配置密钥时构建失败。
npx tauri build --target "$TARGET" --bundles app --config src-tauri/tauri.ci.conf.json

APP="$BUNDLE/macos/Chatters.app"
[ -d "$APP" ] || {
    printf 'ERROR: 未找到 app bundle：%s\n' "$APP" >&2
    exit 1
}

VERSION="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json | head -1)"
[ -n "$VERSION" ] || VERSION="unknown"

mkdir -p "$OUT_DIR"
STAGE="$(mktemp -d)"
cleanup() { rm -rf "$STAGE"; }
trap cleanup EXIT

cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

DMG="$OUT_DIR/Chatters-${VERSION}-macos-${ARCH_TAG}.dmg"
ZIP="$OUT_DIR/Chatters-${VERSION}-macos-${ARCH_TAG}.app.zip"

echo "==> 生成 DMG（hdiutil，无 Finder 依赖）"
hdiutil create -volname "Chatters ${VERSION}" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
hdiutil verify "$DMG" | tail -1

echo "==> 生成 app.zip"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"

(
    cd "$OUT_DIR"
    shasum -a 256 ./*.dmg ./*.app.zip > SHA256SUMS.txt
    cat SHA256SUMS.txt
    ls -lh ./*.dmg ./*.app.zip
)
