#!/usr/bin/env bash
# Chatters 开发环境自检（只读，不修改系统）。
#
# 用法：bash tools/check-env.sh
# 退出码：0 = 必需项全部通过；1 = 存在必需项失败。
# 说明：判定标准见 requirements/18-environment-setup.md。

set -u

FAILS=0
WARNS=0

pass() { printf '  ✅ %s\n' "$1"; }
fail() { printf '  ❌ %s\n' "$1"; FAILS=$((FAILS + 1)); }
warn() { printf '  ⚠️  %s\n' "$1"; WARNS=$((WARNS + 1)); }
note() { printf '     %s\n' "$1"; }

# version_ge A B：A >= B 时返回 0（按 3 段数字比较，缺省段补 0）。
version_ge() {
    awk -v a="$1" -v b="$2" 'BEGIN{
        n = split(a, x, "."); m = split(b, y, ".");
        for (i = 1; i <= 3; i++) {
            xi = (i <= n) ? x[i] + 0 : 0;
            yi = (i <= m) ? y[i] + 0 : 0;
            if (xi > yi) exit 0;
            if (xi < yi) exit 1;
        }
        exit 0;
    }'
}

# node_state V：0 = 满足锁定依赖 engines（^22.22.2 || ^24.15.0 || >=26）；
#               1 = 满足 Vite 7 下限但会 EBADENGINE；2 = 不满足。
node_state() {
    local v="$1" major
    major="${v%%.*}"
    if [ "$major" -ge 26 ]; then
        return 0
    fi
    if [ "$major" -eq 24 ] && version_ge "$v" "24.15.0"; then
        return 0
    fi
    if [ "$major" -eq 22 ] && version_ge "$v" "22.22.2"; then
        return 0
    fi
    if version_ge "$v" "20.19.0"; then
        return 1
    fi
    return 2
}

echo "== Chatters 环境自检 =="
printf '  工作目录：%s\n' "$(pwd)"
printf '  系统：%s %s（%s）\n\n' "$(uname -s)" "$(uname -r)" "$(uname -m)"

# ── Node / npm ────────────────────────────────────────────────
echo "[Node.js]"
if command -v node >/dev/null 2>&1; then
    NODE_VER="$(node -v 2>/dev/null | sed 's/^v//')"
    node_state "$NODE_VER"
    NODE_STATE=$?
    if [ "$NODE_STATE" -eq 0 ]; then
        pass "node v${NODE_VER}（满足锁定依赖 engines）"
    elif [ "$NODE_STATE" -eq 1 ]; then
        warn "node v${NODE_VER} 可运行，但 abbrev/nopt 要求 ^22.22.2 || ^24.15.0 || >=26，npm ci 会报 EBADENGINE"
        note "推荐：nvm use 22.22.2（本机已安装）"
    else
        fail "node v${NODE_VER} 版本过低（Vite 7 需 >=20.19 或 >=22.12）"
        note "修复：nvm use 22.22.2 && nvm alias default 22.22.2（然后重开终端）"
    fi
else
    fail "未找到 node（安装 Node 22.22.2 LTS）"
fi

if command -v npm >/dev/null 2>&1; then
    NPM_VER="$(npm -v 2>/dev/null)"
    if version_ge "$NPM_VER" "10.0.0"; then
        pass "npm $NPM_VER"
    else
        fail "npm $NPM_VER 版本过低（需 >=10，随 Node 22 自带）"
        note "修复：先切换 Node 22，再确认 npm -v"
    fi
else
    fail "未找到 npm"
fi

# ── Rust 工具链 ───────────────────────────────────────────────
echo "[Rust]"
if command -v rustc >/dev/null 2>&1; then
    RUST_VER="$(rustc --version | awk '{print $2}')"
    if version_ge "$RUST_VER" "1.77.2"; then
        pass "rustc ${RUST_VER}（Tauri 2 最低 1.77.2）"
    else
        fail "rustc $RUST_VER 版本过低（需 >=1.77.2）"
    fi
else
    fail "未找到 rustc（安装 Rust stable）"
fi

if command -v cargo >/dev/null 2>&1; then
    pass "cargo $(cargo --version | awk '{print $2}')"
else
    fail "未找到 cargo"
fi

if command -v rustfmt >/dev/null 2>&1; then
    pass "rustfmt $(rustfmt --version | awk '{print $2}')"
else
    fail "未找到 rustfmt（门禁 cargo fmt 需要）"
    note "修复：rustup component add rustfmt（Homebrew 版可 brew install rustfmt）"
fi

if cargo clippy --version >/dev/null 2>&1; then
    pass "$(cargo clippy --version | head -1)"
else
    fail "cargo clippy 不可用（门禁 clippy -D warnings 需要）"
    note "修复：rustup component add clippy"
fi

# ── macOS 构建依赖 ────────────────────────────────────────────
echo "[macOS 构建依赖]"
if [ "$(uname -s)" = "Darwin" ]; then
    if xcode-select -p >/dev/null 2>&1; then
        pass "Xcode 命令行工具：$(xcode-select -p)"
    else
        fail "未安装 Xcode Command Line Tools"
        note "修复：xcode-select --install"
    fi

    if SDK_PATH="$(xcrun --show-sdk-path 2>/dev/null)" && [ -n "$SDK_PATH" ]; then
        pass "macOS SDK：$(xcrun --show-sdk-version 2>/dev/null)（${SDK_PATH}）"
    else
        fail "无法定位 macOS SDK（检查 Xcode 许可：sudo xcodebuild -license accept）"
    fi
else
    warn "非 macOS 平台：请按 requirements/18 §6.3 安装对应系统依赖"
fi

if command -v git >/dev/null 2>&1; then
    pass "$(git --version)"
else
    fail "未找到 git"
fi

# ── 端口 ─────────────────────────────────────────────────────
echo "[端口]"
check_port() {
    local proto="$1" port="$2" desc="$3" in_use=0
    if [ "$proto" = "UDP" ]; then
        lsof -nP -i"$proto:$port" >/dev/null 2>&1 && in_use=1
    else
        lsof -nP -i"$proto:$port" -sTCP:LISTEN >/dev/null 2>&1 && in_use=1
    fi

    if [ "$in_use" -eq 1 ]; then
        fail "${proto} ${port}（${desc}）已被占用"
        note "排查：lsof -nP -i$proto:$port"
    else
        pass "${proto} ${port}（${desc}）空闲"
    fi
}
check_port TCP 5173 "Vite dev"
check_port UDP 28100 "设备发现"
check_port TCP 28101 "会话通信"

# ── 依赖源连通性 ─────────────────────────────────────────────
echo "[依赖源]"
if curl -fsSI --max-time 8 https://registry.npmmirror.com/vite >/dev/null 2>&1; then
    pass "npm 源 registry.npmmirror.com 可达"
elif curl -fsSI --max-time 8 https://registry.npmjs.org/vite >/dev/null 2>&1; then
    warn "registry.npmmirror.com 不可达，但 registry.npmjs.org 可达"
    note "可用：npm ci --registry=https://registry.npmjs.org"
else
    fail "npm 源不可达（首次安装依赖需要联网）"
fi

if curl -fsSI --max-time 8 https://index.crates.io/config.json >/dev/null 2>&1; then
    pass "crates.io 索引可达"
else
    fail "crates.io 索引不可达（首次 cargo fetch 需要联网）"
fi

# ── 工程状态（提示项） ───────────────────────────────────────
echo "[工程状态]"
if [ -d node_modules ]; then
    pass "node_modules 已存在"
else
    warn "node_modules 不存在：先执行 npm ci"
fi

if [ -d src-tauri/target ]; then
    pass "src-tauri/target 已存在（Rust 已编译过）"
else
    warn "src-tauri/target 不存在：首次 npm run tauri dev 需编译数分钟"
fi

# ── 可选：Android 构建 ───────────────────────────────────────
echo "[可选：Android]"
ANDROID_SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
if [ -d "$ANDROID_SDK/platform-tools" ]; then
    pass "Android SDK：${ANDROID_SDK}"
    if [ -d "$ANDROID_SDK/platforms/android-36" ]; then
        pass "platforms;android-36"
    else
        warn "缺少 platforms;android-36"
    fi
    if [ -d "$ANDROID_SDK/build-tools/36.0.0" ]; then
        pass "build-tools;36.0.0"
    else
        warn "缺少 build-tools;36.0.0"
    fi
    if [ -d "$ANDROID_SDK/ndk/27.0.12077973" ]; then
        pass "ndk;27.0.12077973"
    else
        warn "缺少 ndk;27.0.12077973"
    fi
else
    warn "未找到 Android SDK（可选：${ANDROID_SDK}）"
fi

if [ -n "${JAVA_HOME:-}" ] && [ -x "$JAVA_HOME/bin/java" ]; then
    JAVA_VER="$("$JAVA_HOME/bin/java" -version 2>&1 | awk -F'"' '/version/ {print $2}')"
    case "$JAVA_VER" in
        17.*) pass "JDK ${JAVA_VER}" ;;
        *) warn "JDK ${JAVA_VER}（AGP 要求 17）" ;;
    esac
elif command -v java >/dev/null 2>&1 && JAVA_VER="$(java -version 2>&1 | awk -F'"' '/version/ {print $2}')" && [ -n "$JAVA_VER" ]; then
    case "$JAVA_VER" in
        17.*) pass "JDK ${JAVA_VER}" ;;
        *) warn "JDK ${JAVA_VER}（AGP 要求 17）" ;;
    esac
else
    warn "未找到 JDK 17（可选：Android 打包需要）"
fi

if command -v rustup >/dev/null 2>&1; then
    if rustup target list --installed 2>/dev/null | grep -qx 'aarch64-linux-android'; then
        pass "Rust target aarch64-linux-android"
    else
        warn "缺少 Rust target：rustup target add aarch64-linux-android"
    fi
else
    warn "未安装 rustup（Android 交叉编译需要）"
fi

if [ -x src-tauri/gen/android/gradlew ]; then
    pass "Gradle wrapper 可执行（gradle-9.6.1）"
else
    warn "src-tauri/gen/android/gradlew 缺少执行位（chmod +x）"
fi

# ── 汇总 ─────────────────────────────────────────────────────
echo
if [ "$FAILS" -eq 0 ]; then
    printf '结果：必需项全部通过（警告 %d 项）。\n' "$WARNS"
    exit 0
fi
printf '结果：%d 项必需环境未满足，%d 项警告。请按上面的提示修复。\n' "$FAILS" "$WARNS"
exit 1
