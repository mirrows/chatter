#!/usr/bin/env bash
# Chatters Android arm64 APK 构建（macOS / Linux）。
#
# 与 tools/build-android-arm64.ps1、.github/workflows/ci.yml 的 android job 等价：
#   1. 校验 Node / JAVA_HOME / ANDROID_HOME / NDK / rustup target
#   2. npx tauri android build --apk --target aarch64（CLI 生成胶水文件并调用 Gradle）
#   3. 收集 APK 到 src-tauri/target/android/
#   4. 未配置发布 keystore 时回退 debug 签名（仅用于侧载）
#
# 用法：
#   bash tools/build-android-arm64.sh            # release 构建（debug 签名回退）
#   bash tools/build-android-arm64.sh --debug    # debug 构建
#
# 环境变量（可选）：
#   JAVA_HOME / ANDROID_HOME / NDK_HOME
#   CHATTERS_KEYSTORE / CHATTERS_KEYSTORE_PASSWORD / CHATTERS_KEY_ALIAS / CHATTERS_KEY_PASSWORD

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

DEBUG_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --debug) DEBUG_BUILD=1 ;;
        -h | --help)
            sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            printf 'ERROR: 未知参数 %s\n' "$arg" >&2
            exit 2
            ;;
    esac
done

die() {
    printf 'ERROR: %s\n' "$*" >&2
    exit 1
}

note() {
    printf '==> %s\n' "$*"
}

# ── Node：锁定依赖要求 22.22.2 / 24.15+ / 26+ ────────────────
command -v node >/dev/null 2>&1 || die "未找到 node（需要 22.22.2 或 24.15+）"
NODE_OK="$(node -e 'const [maj, min, pat] = process.versions.node.split(".").map(Number);
const ok = (maj === 22 && (min > 22 || (min === 22 && pat >= 2)))
        || (maj === 24 && (min > 15 || (min === 15 && pat >= 0)))
        || maj >= 26;
process.stdout.write(ok ? "yes" : "no");')"
if [ "$NODE_OK" != "yes" ]; then
    die "node $(node -v) 版本不满足要求（22.22.2 / 24.15+ / 26+）；先执行 nvm use 22.22.2"
fi
note "node $(node -v)"

# ── Rust：需要 rustup 管理的 aarch64-linux-android target ─────
export PATH="$HOME/.cargo/bin:$PATH"
command -v rustup >/dev/null 2>&1 || die "未找到 rustup（brew install rustup 或官方 rustup-init）"
if ! rustup target list --installed 2>/dev/null | grep -qx aarch64-linux-android; then
    die "缺少 Rust target：执行 rustup target add aarch64-linux-android"
fi
note "rustc $(rustc --version | awk '{print $2}')（$(rustup show active-toolchain 2>/dev/null | awk '{print $1}')）"

# ── JDK 17 ─────────────────────────────────────────────────
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "${JAVA_HOME}/bin/java" ]; then
    if [ -x /opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home/bin/java ]; then
        JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home
    elif [ -x /usr/local/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home/bin/java ]; then
        JAVA_HOME=/usr/local/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home
    elif [ "$(uname -s)" = "Darwin" ] && /usr/libexec/java_home -v 17 >/dev/null 2>&1; then
        JAVA_HOME="$(/usr/libexec/java_home -v 17)"
    fi
fi
[ -n "${JAVA_HOME:-}" ] && [ -x "${JAVA_HOME}/bin/java" ] \
    || die "未找到 JDK 17（brew install openjdk@17 并设置 JAVA_HOME）"
export JAVA_HOME

# ── Android SDK / NDK ──────────────────────────────────────
if [ -z "${ANDROID_HOME:-}" ]; then
    if [ -d "$HOME/Library/Android/sdk" ]; then
        ANDROID_HOME="$HOME/Library/Android/sdk"
    elif [ -d "$HOME/Android/Sdk" ]; then
        ANDROID_HOME="$HOME/Android/Sdk"
    fi
fi
[ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME" ] \
    || die "未找到 Android SDK（设置 ANDROID_HOME）"
export ANDROID_HOME
export ANDROID_SDK_ROOT="$ANDROID_HOME"

BUILD_TOOLS="$ANDROID_HOME/build-tools/36.0.0"
[ -d "$ANDROID_HOME/platforms/android-36" ] || die "缺少 platforms;android-36（sdkmanager 安装）"
[ -d "$BUILD_TOOLS" ] || die "缺少 build-tools;36.0.0（sdkmanager 安装）"

if [ -z "${NDK_HOME:-}" ] || [ ! -d "$NDK_HOME" ]; then
    if [ -d "$ANDROID_HOME/ndk/27.0.12077973" ]; then
        NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
    else
        NDK_HOME="$(find "$ANDROID_HOME/ndk" -maxdepth 1 -mindepth 1 -type d 2>/dev/null | sort | tail -1)"
    fi
fi
[ -n "${NDK_HOME:-}" ] && [ -d "$NDK_HOME" ] || die "未找到 Android NDK（sdkmanager 安装 ndk;27.0.12077973）"
export NDK_HOME
export ANDROID_NDK_HOME="$NDK_HOME"

export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$BUILD_TOOLS:$PATH"

note "JAVA_HOME=$JAVA_HOME"
note "ANDROID_HOME=$ANDROID_HOME"
note "NDK_HOME=$NDK_HOME"

# ── 构建 ───────────────────────────────────────────────────
BUILD_ARGS=(--apk --target aarch64)
if [ "$DEBUG_BUILD" -eq 1 ]; then
    BUILD_ARGS+=(--debug)
    note "构建 debug APK（arm64）"
else
    note "构建 release APK（arm64）"
fi
npx tauri android build "${BUILD_ARGS[@]}"

# ── 产物收集 / 签名 / 校验 ──────────────────────────────────
VERSION="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json | head -1)"
[ -n "$VERSION" ] || VERSION="unknown"

APK_DIR="src-tauri/gen/android/app/build/outputs/apk"
OUT_DIR="src-tauri/target/android"
mkdir -p "$OUT_DIR"

if [ "$DEBUG_BUILD" -eq 1 ]; then
    APK="$(find "$APK_DIR" -path '*debug/app-*debug.apk' 2>/dev/null | head -1)"
    [ -n "$APK" ] || die "未找到 debug APK（${APK_DIR}）"
    FINAL="$OUT_DIR/Chatters-${VERSION}-arm64-debug.apk"
    cp "$APK" "$FINAL"
else
    SIGNED="$(find "$APK_DIR" -path '*release/app-*-release.apk' ! -name '*-unsigned.apk' 2>/dev/null | head -1)"
    if [ -n "$SIGNED" ]; then
        FINAL="$OUT_DIR/Chatters-${VERSION}-arm64-release.apk"
        cp "$SIGNED" "$FINAL"
    else
        UNSIGNED="$(find "$APK_DIR" -path '*release/app-*-release-unsigned.apk' 2>/dev/null | head -1)"
        [ -n "$UNSIGNED" ] || die "未找到 APK 产物（${APK_DIR}）"

        DEBUG_KEYSTORE="${HOME}/.android/debug.keystore"
        if [ ! -f "$DEBUG_KEYSTORE" ]; then
            note "生成 debug keystore：$DEBUG_KEYSTORE"
            mkdir -p "$(dirname "$DEBUG_KEYSTORE")"
            keytool -genkeypair -keystore "$DEBUG_KEYSTORE" -storepass android \
                -alias androiddebugkey -keypass android \
                -dname "CN=Android Debug,O=Android,C=US" \
                -keyalg RSA -keysize 2048 -validity 10000 >/dev/null
        fi

        ALIGNED="$OUT_DIR/Chatters-${VERSION}-arm64-aligned.apk"
        FINAL="$OUT_DIR/Chatters-${VERSION}-arm64-debugsigned.apk"
        note "zipalign + debug 签名（仅侧载，不可上架）"
        "$BUILD_TOOLS/zipalign" -p -f 4 "$UNSIGNED" "$ALIGNED"
        "$BUILD_TOOLS/apksigner" sign --ks "$DEBUG_KEYSTORE" \
            --ks-pass pass:android --key-pass pass:android \
            --out "$FINAL" "$ALIGNED"
        rm -f "$ALIGNED"
    fi
fi

"$BUILD_TOOLS/apksigner" verify --verbose --print-certs "$FINAL" | head -8
if command -v unzip >/dev/null 2>&1; then
    # 先落盘再 grep：避免 grep -q 提前退出导致 unzip 收到 SIGPIPE，
    # 在 `set -o pipefail` 下被误判为失败。
    LIST_FILE="$(mktemp)"
    unzip -Z1 "$FINAL" > "$LIST_FILE"
    if ! grep -qx 'lib/arm64-v8a/libchatters_lib.so' "$LIST_FILE"; then
        rm -f "$LIST_FILE"
        die "APK 中缺少 lib/arm64-v8a/libchatters_lib.so"
    fi
    rm -f "$LIST_FILE"
fi

note "产物：${FINAL}（$(du -h "$FINAL" | awk '{print $1}')）"
if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$FINAL"
else
    sha256sum "$FINAL"
fi

