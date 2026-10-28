#!/usr/bin/env bash
# 多架构发布二进制构建脚本（macOS 开发机上交叉编 linux / windows）
#
# 配套说明与实测矩阵见同目录 build-binaries.md。
#
# 用法：
#   docs/runbook/build-binaries.sh --check
#   docs/runbook/build-binaries.sh --target x86_64-unknown-linux-gnu
#   docs/runbook/build-binaries.sh --all
#
# 环境变量：
#   ZIG=<路径>       zig 可执行文件（默认取 PATH 上的 zig）
#   DIST=<目录>      产物输出目录（默认 <repo>/dist）
#   GLIBC=x.y        把 glibc 需求钉到指定版本（如 2.28），不钉则用 zig 默认
#   KEEP_LOGS=1      保留每个 target 的构建日志

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="${DIST:-$ROOT/dist}"
CACHE="${CACHE:-$ROOT/.runbook-cache}"
LOGDIR="${LOGDIR:-${TMPDIR:-/tmp}/tabby-build-logs}"
GLIBC="${GLIBC:-}"
# 默认 toolchain 的 cross std 下载失败时，可指向本机已带该 std 的旧 toolchain，例如 1.94.0
TOOLCHAIN="${TOOLCHAIN:-}"
declare -a CARGO_CMD=(cargo)
if [ -n "$TOOLCHAIN" ]; then CARGO_CMD=(cargo "+$TOOLCHAIN"); fi
LLAMA=1
REFRESH_UI=0
PROFILE="release"
declare -a TARGETS=()

ALL_TARGETS=(
  aarch64-apple-darwin
  x86_64-apple-darwin
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
  x86_64-pc-windows-gnu
)

die() { echo "error: $*" >&2; exit 1; }
say() { echo "[build] $*"; }

usage() {
  sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  cat <<'EOF'

选项：
  --target <triple>   指定目标（可重复）
  --all               构建 ALL_TARGETS 中所有前置条件满足的目标
  --check             只打印每个 target 的就绪状态，不构建
  --skip-llama-server 只出 tabby 主程序
  --debug             用 dev profile（快，但不可用于发布）
  --refresh-ui        构建前先跑 pnpm build + 同步 ee/tabby-webserver/ui
EOF
}

# ---------- target 知识 ----------
is_apple() { case "$1" in *-apple-darwin) return 0 ;; esac; return 1; }
is_windows() { case "$1" in *-pc-windows-*) return 0 ;; esac; return 1; }
exe_ext() { is_windows "$1" && echo ".exe" || echo ""; }

# rust triple -> zig triple
zig_triple() {
  case "$1" in
    x86_64-unknown-linux-gnu)   echo "x86_64-linux-gnu" ;;
    aarch64-unknown-linux-gnu)  echo "aarch64-linux-gnu" ;;
    x86_64-unknown-linux-musl)  echo "x86_64-linux-musl" ;;
    aarch64-unknown-linux-musl) echo "aarch64-linux-musl" ;;
    x86_64-pc-windows-gnu)      echo "x86_64-windows-gnu" ;;
    aarch64-pc-windows-gnu)     echo "aarch64-windows-gnu" ;;
    *) echo "" ;;
  esac
}

# cargo --target 实参：linux-gnu 可选钉 glibc 版本（cargo-zigbuild 支持 triple-<major>.<minor> 语法）
cargo_target_arg() {
  local t="$1"
  case "$(zig_triple "$t")" in
    *-linux-gnu) if [ -n "$GLIBC" ]; then echo "${t}.${GLIBC}"; else echo "$t"; fi ;;
    *) echo "$t" ;;
  esac
}

# 钉了 glibc 时 target 目录可能带 .x.y 后缀，也可能被 cargo 归一掉，两个都试
resolve_bin() {
  local ct="$1" name="$2" base="${1%%.*}" ext="$3" cand
  for cand in "target/$ct/$PROFILE/$name$ext" "target/$base/$PROFILE/$name$ext"; do
    [ -f "$ROOT/$cand" ] && { echo "$ROOT/$cand"; return 0; }
  done
  return 1
}

# ---------- 前置条件 ----------
locate_zig() {
  ZIG="${ZIG:-$(command -v zig || true)}"
  [ -n "$ZIG" ] && [ -x "$ZIG" ] && return 0
  return 1
}

has_rustup_target() {
  if [ -n "$TOOLCHAIN" ]; then
    rustup +$TOOLCHAIN target list --installed 2>/dev/null | grep -qx "$1"
  else
    rustup target list --installed 2>/dev/null | grep -qx "$1"
  fi
}

check_target() {
  local t="$1" reason=""
  if is_apple "$t"; then
    has_rustup_target "$t" || reason="rustup target add $t"
  else
    locate_zig || reason="zig 未安装（见 build-binaries.md 的「安装 zig」）"
    has_rustup_target "$t" || reason="${reason:+$reason; }rustup target add $t"
    # 注意：windows 的 llama-server 不通是「少一个二进制」，不是「整个 target 不能做」，
    # 所以它只作为提示（见 build_one），不能进 skip 理由
  fi
  echo "$reason"
}

cmd_check() {
  locate_zig && say "zig: $ZIG ($("$ZIG" version 2>/dev/null | head -1))" || say "zig: 未找到"
  printf '%-30s %-8s %s\n' "TARGET" "就绪?" "缺什么"
  for t in "${ALL_TARGETS[@]}"; do
    local r; r="$(check_target "$t")"
    if [ -z "$r" ]; then
      if is_windows "$t"; then printf '%-30s %-8s %s\n' "$t" "yes" "llama-server 交叉未通过，只出 tabby（见文档「已知不通」）"
      else printf '%-30s %-8s %s\n' "$t" "yes" "-"; fi
    else printf '%-30s %-8s %s\n' "$t" "no" "$r"; fi
  done
}

# ---------- UI 新鲜度 ----------
ui_is_stale() {
  local marker="$ROOT/ee/tabby-webserver/ui/index.html"
  [ -f "$marker" ] || return 0
  [ -n "$(find "$ROOT/ee/tabby-ui" -type f \
      -not -path '*/node_modules/*' -not -path '*/.next/*' -not -path '*/out/*' \
      -newer "$marker" -print -quit 2>/dev/null)" ]
}

refresh_ui() {
  say "重建前端产物（pnpm build）"
  command -v pnpm >/dev/null || die "需要 pnpm"
  ( cd "$ROOT" && pnpm build )
  [ -d "$ROOT/ee/tabby-ui/out" ] || die "ee/tabby-ui/out 不存在，pnpm build 未产出静态站"
  rm -rf "$ROOT/ee/tabby-webserver/ui"
  cp -R "$ROOT/ee/tabby-ui/out" "$ROOT/ee/tabby-webserver/ui"
  say "ee/tabby-webserver/ui 已更新（rust-embed 将在下次 cargo build 时嵌入）"
}

# ---------- 生成 zig 的 cmake toolchain ----------
ensure_toolchain() {
  local t="$1" zt; zt="$(zig_triple "$t")"
  local zdir; zdir="$(dirname "$ZIG")"
  local zb; zb="$("$ZIG" version | head -1)"
  local bin="$CACHE/zigbin-$zt"
  mkdir -p "$bin"
  for c in cc c++; do
    printf '#!/bin/sh\nexec "%s" %s -target %s "$@"\n' "$ZIG" "$c" "$zt" > "$bin/$zt-$c"
    chmod +x "$bin/$zt-$c"
  done
  for c in ar ranlib; do
    printf '#!/bin/sh\nexec "%s" %s "$@"\n' "$ZIG" "$c" > "$bin/$zt-$c"
    chmod +x "$bin/$zt-$c"
  done
  local sys=Linux proc="${zt%%-*}"
  cat > "$CACHE/$zt.cmake" <<EOF
set(CMAKE_SYSTEM_NAME ${sys})
set(CMAKE_SYSTEM_PROCESSOR ${proc})
set(CMAKE_C_COMPILER   "${bin}/${zt}-cc")
set(CMAKE_CXX_COMPILER "${bin}/${zt}-c++")
set(CMAKE_AR           "${bin}/${zt}-ar")
set(CMAKE_RANLIB       "${bin}/${zt}-ranlib")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
EOF
  echo "$CACHE/$zt.cmake"
}

# ---------- 构建 ----------
feature_args() {
  local t="$1"
  if is_apple "$t"; then
    # 同架构（本机 arch 的 apple target）跟官方 CI 一致，用系统/homebrew 的 openssl；
    # 跨架构时 homebrew 里根本没有对应 arch 的 openssl，openssl-sys 会直接 panic
    # 「Could not find directory of OpenSSL installation」，只能走 vendored 静态编。
    local host_arch; host_arch="$(uname -m)"; [ "$host_arch" = arm64 ] && host_arch=aarch64
    if [ "${t%%-*}" != "$host_arch" ]; then
      echo "--no-default-features --features prod,static-ssl"
    else
      echo "--no-default-features --features prod"
    fi
  else
    echo "--no-default-features --features prod,static-ssl"
  fi
}

build_tabby() {
  local t="$1" ct; ct="$(cargo_target_arg "$t")"
  local profile_flag="--release"
  [ "$PROFILE" = release ] || profile_flag=""
  # sqlx 的 query_as! 宏从 cwd 读 .env 拿 DATABASE_URL，必须在 workspace 根执行
  cd "$ROOT"
  if is_apple "$t"; then
    say "tabby -> $t (cargo build + 宿主 SDK，features: $(feature_args "$t"))"
    # shellcheck disable=SC2086
    "${CARGO_CMD[@]}" build $profile_flag --target "$ct" -p tabby $(feature_args "$t") \
      2>&1 | tee "$LOGDIR/tabby-$t.log"
  else
    say "tabby -> $t (cargo zigbuild + static-ssl)"
    # shellcheck disable=SC2086
    PATH="$(dirname "$ZIG"):$PATH" "${CARGO_CMD[@]}" zigbuild $profile_flag --target "$ct" -p tabby $(feature_args "$t") \
      2>&1 | tee "$LOGDIR/tabby-$t.log"
  fi
  TABBY_BIN="$(resolve_bin "$ct" tabby "$(exe_ext "$t")")" \
    || die "未找到 tabby 产物：target/$ct/$PROFILE/tabby$(exe_ext "$t")"
  say "  tabby: $TABBY_BIN"
}

build_llama_mac() {
  local t="$1"
  local profile_flag="--release"
  [ "$PROFILE" = release ] || profile_flag=""
  say "llama-server -> $t (cargo feature=binary，走 build.rs 的 cmake+Metal)"
  # 必须显式 --target：omnicopy_to_output 靠 OUT_DIR 里的 target/<triple>/<profile>
  # 路径段定位产物目录，不传 --target 时该段不存在，二进制不会落到 target 下
  # shellcheck disable=SC2086
  "${CARGO_CMD[@]}" build --target "$t" $profile_flag -p llama-cpp-server --features binary \
    2>&1 | tee "$LOGDIR/llama-$t.log"
  LLAMA_BIN="$(resolve_bin "$t" llama-server "")" \
    || die "未找到 llama-server 产物：target/$t/$PROFILE/llama-server"
  say "  llama-server: $LLAMA_BIN"
}

build_llama_cross() {
  local t="$1" tc; tc="$(ensure_toolchain "$t")"
  local src="$ROOT/crates/llama-cpp-server/llama.cpp"
  local bdir="$CACHE/cmake-$t"
  [ -d "$src/ggml" ] || die "submodule 未 checkout：git submodule update --init --recursive"
  say "llama-server -> $t (cmake + zig toolchain，CPU 后端)"
  cmake -S "$src" -B "$bdir" \
    -DCMAKE_TOOLCHAIN_FILE="$tc" \
    -DCMAKE_BUILD_TYPE=Release \
    -DLLAMA_CURL=OFF -DGGML_NATIVE=OFF -DBUILD_SHARED_LIBS=OFF \
    -DGGML_METAL=OFF -DGGML_CUDA=OFF -DGGML_VULKAN=OFF -DGGML_BLAS=OFF -DGGML_OPENMP=OFF \
    -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF \
    > "$LOGDIR/llama-$t-configure.log" 2>&1 || { tail -20 "$LOGDIR/llama-$t-configure.log"; die "cmake configure 失败，日志 $LOGDIR/llama-$t-configure.log"; }
  cmake --build "$bdir" --target llama-server -j "${JOBS:-4}" > "$LOGDIR/llama-$t-build.log" 2>&1 \
    || { tail -20 "$LOGDIR/llama-$t-build.log"; die "cmake build 失败，日志 $LOGDIR/llama-$t-build.log"; }
  LLAMA_BIN="$bdir/bin/llama-server"
  [ -x "$LLAMA_BIN" ] || die "未找到 llama-server 产物：$LLAMA_BIN"
}

# 产物架构自检：cp 之前先确认二进制的 arch 与 target 一致。
# 存在的真实失败模式：llama-cpp-server 的 build.rs 不传 CMAKE_OSX_ARCHITECTURES，
# 给 x86_64-apple-darwin 编时 cmake 仍按宿主 arch 出 arm64，cargo 却把它当目标产物搬走，
# 于是包里是一个「架构不对、装得上跑不了」的二进制。
assert_arch() {
  local bin="$1" t="$2" want got
  case "$t" in
    x86_64-*)      want="x86-64|x86_64" ;;
    aarch64-*)     want="arm64|aarch64" ;;
    *) die "assert_arch: 无法识别 target 架构：$t" ;;
  esac
  got="$(file -b "$bin")"
  if ! echo "$got" | grep -qE "$want"; then
    echo "  期望 arch: $want" >&2
    die "产物架构不符：$bin 实际是 -> $got"
  fi
  say "  arch 自检通过：$(basename "$bin") ($t)"
}

package() {
  # 拆成两条 local：bash 在同一语句里先按顺序把名字全部声明为 local（未赋值），
  # 再逐个求值，所以 `local name=... stage="$DIST/$name"` 在 set -u 下会炸
  local t="$1"
  local name="tabby_$t"
  local stage="$DIST/$name"
  local ext; ext="$(exe_ext "$t")"
  rm -rf "$stage"; mkdir -p "$stage"
  cp "$TABBY_BIN" "$stage/tabby$ext"
  [ -n "${LLAMA_BIN:-}" ] && cp "$LLAMA_BIN" "$stage/llama-server$ext"
  chmod +x "$stage"/* 2>/dev/null || true
  ( cd "$DIST"
    if [ -n "$ext" ]; then zip -qr "$name.zip" "$name"; else tar czf "$name.tar.gz" "$name"; fi
    rm -rf "$name" )
  local art="$DIST/$name$([ -n "$ext" ] && echo .zip || echo .tar.gz)"
  ( cd "$DIST" && shasum -a 256 "$(basename "$art")" > "$(basename "$art").sha256" )
  say "打包完成：$art"
  cat "$DIST/$(basename "$art").sha256"
}

build_one() {
  local t="$1" reason; reason="$(check_target "$t")"
  if [ -n "$reason" ]; then say "跳过 $t：$reason"; return 0; fi
  say "=== $t ==="
  TABBY_BIN=""; LLAMA_BIN=""
  # check_target 里的 locate_zig 跑在命令替换的子 shell 里，ZIG 不会带回父进程；
  # 构建路径要用的地方必须自己再定位一次
  if ! is_apple "$t"; then locate_zig || die "需要 zig，见文档「安装 zig」或显式传 ZIG=<路径>"; fi
  build_tabby "$t"
  if [ "$LLAMA" = 1 ]; then
    if is_windows "$t"; then
      say "跳过 llama-server（windows 交叉未通过，只出 tabby）"
    elif is_apple "$t"; then
      build_llama_mac "$t"
    else
      build_llama_cross "$t"
    fi
  fi
  assert_arch "$TABBY_BIN" "$t"
  [ -n "${LLAMA_BIN:-}" ] && assert_arch "$LLAMA_BIN" "$t"
  package "$t"
}


# ---------- 参数 ----------
mkdir -p "$LOGDIR"
while [ $# -gt 0 ]; do
  case "$1" in
    --target) TARGETS+=("$2"); shift 2 ;;
    --target=*) TARGETS+=("${1#*=}"); shift ;;
    --all) TARGETS=("${ALL_TARGETS[@]}"); shift ;;
    --check) cmd_check; exit 0 ;;
    --skip-llama-server) LLAMA=0; shift ;;
    --debug) PROFILE="debug"; shift ;;
    --refresh-ui) REFRESH_UI=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "未知参数：$1（--help）" ;;
  esac
done

[ ${#TARGETS[@]} -gt 0 ] || { usage; exit 1; }
for t in "${TARGETS[@]}"; do
  case " ${ALL_TARGETS[*]} " in *" $t "*) ;; *) die "未知 target：$t（可选：${ALL_TARGETS[*]}）" ;; esac
done

cd "$ROOT"
if [ "$REFRESH_UI" = 1 ]; then refresh_ui; elif ui_is_stale; then
  say "警告：ee/tabby-webserver/ui 比 ee/tabby-ui 源码旧，产物会嵌入旧界面"
  say "      加 --refresh-ui 让本脚本顺手重建，或手工 make update-ui"
fi
say "workspace 版本：$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
if [ -n "$TOOLCHAIN" ]; then RUSTC_INFO="$(rustup run $TOOLCHAIN rustc --version)"; else RUSTC_INFO="$(rustc --version)"; fi
say "rustc：$RUSTC_INFO（官方 release.yml 钉 1.82.0，仓库无 rust-toolchain 文件）"

for t in "${TARGETS[@]}"; do build_one "$t"; done

say "全部完成，产物在 $DIST"
# 用 if 而不是 `[ ... ] && say`：后者在条件为假时会把脚本退出码变成 1
if [ "${KEEP_LOGS:-}" = 1 ]; then say "日志：$LOGDIR"; fi
exit 0
