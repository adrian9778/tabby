# 多架构发布二进制构建 runbook

> **基准**：分支 `adrian` @ `a7863e34`，workspace 版本 `0.33.0-dev.0`
> **实测环境**：macOS 27.0.1 / Apple Silicon (arm64)，rustc 1.99.0，cmake 4.4.4，zig 0.17.0，cargo-zigbuild 0.24.0
> **事实红线**：下表「实测」列只写本轮真的跑过的；未跑的一律标 `未测`，不拿通识冒充本仓库结论
> **配套脚本**：同目录 [`build-binaries.sh`](build-binaries.sh)

---

## 0. 这篇回答什么

从这台 mac 出发，产出能在 **linux / windows / macOS 的 x86_64 与 aarch64 上运行**的 tabby 二进制。

一句话结论：**这台 mac 上已经用脚本一条龙实测出 6 个 release 产物（`dist/` 里 6 个包见 §1），覆盖 macOS 两架构（含 Metal 版 llama-server）、linux gnu 两架构（含 CPU 版 llama-server）、linux musl x86_64（全静态，含 llama-server）、windows x86_64（只有 tabby）；没打通的只有 windows 版 llama-server，未测的是 aarch64-musl 与 aarch64-windows。**

四个前提条件不是「知道就好」，是本轮真的卡过：交叉必须带 `static-ssl`（§7.2）、release 必须用默认 toolchain（§7.7）、windows 还需要 `nasm`（§7.8）、跨 arch 的苹果目标同样要 `static-ssl`（§7.9）。

---

## 1. 结论速览：目标矩阵

| target | tabby 主程序 | llama-server | 状态 |
|---|---|---|---|
| `aarch64-apple-darwin` | 宿主原生 | Metal（cmake） | **`实测` 双通**（脚本一条龙出 `dist/tabby_aarch64-apple-darwin.tar.gz` 32MB；llama-server 8.5MB Mach-O arm64，`otool -L` 含 Metal/MetalKit/Accelerate） |
| `x86_64-apple-darwin` | 宿主 SDK 可直接编 | Metal | **`实测` 双通**（release：tabby 106MB x86_64 Mach-O **vendored openssl** + llama-server 8.6MB x86_64 Mach-O 带 Metal；跨 arch 苹果目标必须 `static-ssl`，见 §7.9） |
| `x86_64-unknown-linux-gnu` | zigbuild `static-ssl` | zig + cmake | **`实测` 双通**（release：tabby 99MB 已 stripped / llama-server 69MB ELF x86-64，脚本打包 45MB tar.gz） |
| `aarch64-unknown-linux-gnu` | zigbuild `static-ssl` | zig + cmake | **`实测` 双通**（release：tabby 98MB stripped ARM aarch64 + llama-server 71MB ARM aarch64 → 44MB tar.gz） |
| `x86_64-unknown-linux-musl` | zigbuild `static-ssl` | zig + cmake | **`实测` 双通（全静态）**（release：tabby 96MB + llama-server 67MB 均 **statically linked** → 45MB tar.gz；glibc 依赖为零，任何 linux 可拷贝运行） |
| `aarch64-unknown-linux-musl` | 同上 | 同上 | std 已装齐（镜像），构建 `未测` |
| `x86_64-pc-windows-gnu` | zigbuild `static-ssl` **+ 需要 nasm** | **不通** | tabby **`实测` 通**（release：PE32+ x86-64，118MB exe → 34MB zip）；llama-server 见 §7.1 |
| `aarch64-pc-windows-gnu` | 同上 | 不通 | std 未装（`rustup target add aarch64-pc-windows-gnu`），构建 `未测` |

四条路线都由 `build-binaries.sh` 一条龙产出（不是手工命令），判据是「被包命令退出码 0 且 `dist/` 里有包」；五条全过架构自检或事后 `file` 复核：

```
dist/tabby_aarch64-apple-darwin.tar.gz         32M   # tabby + llama-server(Metal)
dist/tabby_x86_64-apple-darwin.tar.gz          32M   # tabby + llama-server(Metal, x86_64)
dist/tabby_x86_64-unknown-linux-gnu.tar.gz     45M   # tabby + llama-server(CPU)
dist/tabby_aarch64-unknown-linux-gnu.tar.gz    44M   # tabby + llama-server(CPU)
dist/tabby_x86_64-unknown-linux-musl.tar.gz    45M   # tabby + llama-server（全静态）
dist/tabby_x86_64-pc-windows-gnu.zip           34M   # 只有 tabby
```

`static-ssl` 的证据（对 release 产物实测）：两个 linux 的 **`tabby`** 产物里 `libssl.so` / `libcrypto.so` 字符串数**都是 0**，能扫到的 `.so` 名字只剩这些（`libnvidia-ml.so` 是可选的 GPU 探测目标，不是链接依赖）：

```
libc.so.6  libdl.so.2  libm.so.6  libpthread.so.0  (libnvidia-ml.so)
```

这是 `static-ssl` 生效的直接证据，也是「产物拷到目标机就能跑」的前提。

---

## 2. 为什么不能「一条命令出全平台」

三个硬边界，跟工具链好坏无关：

1. **Metal 是 macOS 独有**。`llama-cpp-server/build.rs` 只在 `target_os = "macos"` 分支里 link Metal/MetalKit framework，linux/windows 上没有对应后端可编。
2. **CUDA 需要目标机上有 nvcc**。上午刚修的那个坑（`GGML_CUDA` 在 mac 上被 `--features cuda` 打开导致 configure 中断）就是这条边界的表现；交叉编译同理拿不到 nvcc。
3. **MSVC 需要 MSVC 工具链**，mac 上不存在。所以本 runbook 的 windows 路线只覆盖 `*-pc-windows-gnu`（zig 能产出合法 PE32+），它与官方发布用的 `x86_64-windows-msvc` **不是同一个 libc ABI**，不能当成官方包等价物分发。

因此：**要 GPU 版 llama-server，必须在那台机器上编，或走 CI**（官方 [.github/workflows/release.yml](../../.github/workflows/release.yml) 就是 mac / manylinux 容器 / windows-latest 三台原生 runner）。本 mac 单机能给的 linux/windows llama-server 只有 CPU 后端。

---

## 3. 两个产物的分工（决定了命令长什么样）

发布用特性组合是 `--no-default-features --features prod`，它**不含** `llama-cpp-server/binary`，所以主程序构建完全不触发 cmake/nvcc：

```bash
# 实测：产物 otool 里 MetalKit 出现次数 = 0
cargo build --no-default-features --features prod -p tabby
```

`llama-server` 是**独立第二个二进制**，靠 `--features binary` 才编（见 release.yml 的 `release-llama-server-binary` job）。两者必须**放在同一目录**交付，依据是运行期查找逻辑 [crates/llama-cpp-server/src/supervisor.rs:224](../../crates/llama-cpp-server/src/supervisor.rs#L224)：扫 `tabby` 可执行文件所在目录里**任何以 `llama-server` 开头**的文件，找不到才回退 `PATH`；都没有就在 `find_binary_name` 处 panic。

---

## 4. 依赖与安装

### 4.1 zig（交叉 C/C++ 编译器 + ar/ranlib/dlltool）

**实测：`brew install zig` 在这台机器上失败**，且失败前已连带拖下 `llvm@22`、`lld@22` 两个大 bottle：

```
Error: Failed to download resource "zig"
curl: (92) HTTP/2 stream 1 reset by server (error 0x1 PROTOCOL_ERROR)
```

走的是 ghcr.io，被本机代理掐断。**推荐用官方 tarball**（本轮实测可用，54MB，自包含、不进 PATH 也不碰 `/opt/homebrew`）：

```bash
curl -sSL https://ziglang.org/download/0.17.0/zig-aarch64-macos-0.17.0.tar.xz -o /tmp/zig.tar.xz
mkdir -p /tmp/zigx && tar -xJf /tmp/zig.tar.xz -C /tmp/zigx --strip-components=1
/tmp/zigx/zig version   # 0.17.0
```

自检它真的能交叉出目标平台的可执行文件：

```bash
printf '#include <stdio.h>\nint main(){return 0;}\n' > /tmp/t.c
/tmp/zigx/zig cc -target x86_64-linux-gnu   /tmp/t.c -o /tmp/t1 && file /tmp/t1
/tmp/zigx/zig cc -target aarch64-linux-musl /tmp/t.c -o /tmp/t2 && file /tmp/t2
/tmp/zigx/zig cc -target x86_64-windows-gnu /tmp/t.c -o /tmp/t3 && file /tmp/t3
```

实测输出分别是 `ELF 64-bit ... x86-64 ... GNU/Linux`、`ELF 64-bit ... ARM aarch64 ... statically linked`、`PE32+ executable (console) x86-64, for MS Windows`。**因为 zig 自带 windows-gnu 所需的一切，本 runbook 不需要 `brew install mingw-w64`。**

### 4.2 rustup target（本轮最大阻塞点）

```bash
rustup target add x86_64-apple-darwin \
  x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu \
  x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
  x86_64-pc-windows-gnu
```

**实测：6 个里只有 `x86_64-unknown-linux-musl` 装成功**，其余下载在代理下超时/失败，且 `rustup target add` 多目标时**一个失败会连坐**。排查时逐个加 `target list --installed` 复核，别信命令的退出码。

根因不是 rustup 的 bug，是 **`static.rust-lang.org` 这台出口对大文件传输有限速**：同一时刻 `curl https://static.rust-lang.org/dist/...` 也只有 ~12KB/s（60 秒才 748K），所以换 curl 直下、手工解包都救不了。

**正解是换镜像**（实测）：

```bash
export RUSTUP_DIST_SERVER=https://rsproxy.cn RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup
rustup target add x86_64-unknown-linux-gnu          # 实测 4.1MB/s，26MB 七秒多下完
```

`mirrors.ustc.edu.cn/rust-static` 同样可用。两个镜像都按 `dist/<date>/` 目录取包，注意直连官网时的 URL 形状是 `dist/2026-10-01/rust-std-1.99.0-<triple>.tar.xz` 而**不是** `dist/1.99.0/...`（写错会 404，本轮踩过）。

> **并且必须装到默认 toolchain 上**：`+1.94.0` 这条「用旧 toolchain 里已有的 std」的绕过路线在 release profile 下有致命问题，见 §7.7。

### 4.2.1 rustup 中断后的永久 conflict（实测踩过）

上一节的超时/被杀下载会在 toolchain 目录里留下**半套 std**：`lib/rustlib/x86_64-apple-darwin/` 与它的 `manifest-...` 文件都在，但内容与实际清单不一致。此后每次安装都失败，而且 `rustup component remove` 报「does not have target installed」——**它按清单认为没装，目录却又存在**，于是死锁：

```
error: failed to install component: 'rust-std-x86_64-apple-darwin',
       detected conflict: 'lib/rustlib/x86_64-apple-darwin/lib/libaddr2line-e7b0d923e824a046.rlib'
```

修法只有把这两个残留挪走再装（实测有效，装完 7 个 target 全绿）：

```bash
TC=~/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib
mkdir -p /tmp/rustup-x86apple-backup
mv "$TC/x86_64-apple-darwin"                              /tmp/rustup-x86apple-backup/dir
mv "$TC/manifest-rust-std-x86_64-apple-darwin"            /tmp/rustup-x86apple-backup/manifest
RUSTUP_DIST_SERVER=https://rsproxy.cn rustup target add x86_64-apple-darwin
```

另注意：**不要并发跑两个 `rustup target add`**。本轮后台里有个上一轮遗留的 rustup 与新的抢同一个 `.partial` 下载文件，报的是 `could not rename 'downloaded' file ... No such file or directory`，看着像网络问题，其实是并发冲突。

### 4.3 其余

| 依赖 | 用途 | 本机状态 |
|---|---|---|
| `cargo-zigbuild` | 驱动 zig 做 Rust 交叉链接 | `cargo install cargo-zigbuild` → 0.24.0 |
| `cmake` | 编 llama.cpp | 4.4.4 |
| `nasm` | **只有 windows 路线要**：`aws-lc-sys` 汇编，见 §7.8 | `brew install nasm` → 3.02（装前 windows 目标必失败） |
| `protoc` | prost 生成 | `/opt/homebrew/bin/protoc` |
| `pnpm` | 重建嵌入 UI | 已装 |
| git submodule | llama.cpp 源码 | `git submodule update --init --recursive` |

---

## 5. 脚本用法

```bash
docs/runbook/build-binaries.sh --check      # 先看每个 target 缺什么，不动源码
docs/runbook/build-binaries.sh --refresh-ui --target x86_64-unknown-linux-gnu
docs/runbook/build-binaries.sh --target aarch64-unknown-linux-gnu --skip-llama-server
ZIG=/tmp/zigx/zig DIST=/tmp/out docs/runbook/build-binaries.sh --all
GLIBC=2.28 docs/runbook/build-binaries.sh --target x86_64-unknown-linux-gnu   # 见 §7.2，未实测
```

`--check` 实测输出（zig 指向 tarball 版、§4.2 的 target 全部装齐之后）：

```
[build] zig: /tmp/zigx/zig (0.17.0)
TARGET                         就绪?  缺什么
aarch64-apple-darwin           yes      -
x86_64-apple-darwin            yes      -
x86_64-unknown-linux-gnu       yes      -
aarch64-unknown-linux-gnu      yes      -
x86_64-unknown-linux-musl      yes      -
aarch64-unknown-linux-musl     yes      -
x86_64-pc-windows-gnu          yes      llama-server 交叉未通过，只出 tabby（见文档「已知不通」）
```

windows 的 llama-server 不通只降级为提示、不挡整个 target——`--target x86_64-pc-windows-gnu` 会正常出 tabby 并在 llama 一步打「跳过」。

产物形态：`dist/tabby_<target>/{tabby,llama-server}` 打成 `.tar.gz`（windows 为 `.zip`），并生成 `.sha256`。

---

## 6. 手工等价命令（不用脚本）

交叉编主程序：

```bash
cd <repo>          # 必须在 workspace 根，原因见 §7.3
PATH=/tmp/zigx:$PATH cargo zigbuild --release \
  --no-default-features --features prod,static-ssl \
  --target x86_64-unknown-linux-gnu -p tabby
```

交叉编 llama-server（zig 当 CMake 的 C/C++/ar/ranlib）：

```bash
# 1) 生成 wrapper：-target 必须烘进脚本里，否则 zig cc 按宿主 arch 编
for c in cc c++; do
  printf '#!/bin/sh\nexec /tmp/zigx/zig %s -target x86_64-linux-gnu "$@"\n' "$c" \
    > /tmp/zt/x86_64-linux-gnu-$c; chmod +x /tmp/zt/x86_64-linux-gnu-$c
done
for c in ar ranlib; do
  printf '#!/bin/sh\nexec /tmp/zigx/zig %s "$@"\n' "$c" > /tmp/zt/x86_64-linux-gnu-$c
  chmod +x /tmp/zt/x86_64-linux-gnu-$c
done

# 2) toolchain 文件
cat > /tmp/zt/linux-x86_64.cmake <<'EOF'
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR x86_64)
set(CMAKE_C_COMPILER   /tmp/zt/x86_64-linux-gnu-cc)
set(CMAKE_CXX_COMPILER /tmp/zt/x86_64-linux-gnu-c++)
set(CMAKE_AR           /tmp/zt/x86_64-linux-gnu-ar)
set(CMAKE_RANLIB       /tmp/zt/x86_64-linux-gnu-ranlib)
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
EOF

# 3) configure + build（GPU 后端全关，CPU only）
cmake -S crates/llama-cpp-server/llama.cpp -B /tmp/lcx \
  -DCMAKE_TOOLCHAIN_FILE=/tmp/zt/linux-x86_64.cmake -DCMAKE_BUILD_TYPE=Release \
  -DLLAMA_CURL=OFF -DGGML_NATIVE=OFF -DBUILD_SHARED_LIBS=OFF \
  -DGGML_METAL=OFF -DGGML_CUDA=OFF -DGGML_VULKAN=OFF -DGGML_BLAS=OFF -DGGML_OPENMP=OFF \
  -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF
cmake --build /tmp/lcx --target llama-server -j 4
file /tmp/lcx/bin/llama-server    # ELF 64-bit LSB executable, x86-64
```

mac 本机的 Metal 版：

```bash
cargo build --release --target aarch64-apple-darwin -p llama-cpp-server --features binary
# 产物在 target/aarch64-apple-darwin/release/llama-server，原因见 §7.4
```

---

## 7. 必须知道的机制与坑

### 7.1 windows 的 llama-server：本轮不通（症状如实记录）

`CMAKE_SYSTEM_NAME=Windows` + 上面同样的 zig toolchain，CMake 的编译器自检阶段就失败：

```
CMake Error at CMakeTestCCompiler.cmake:65 (message):
  The C compiler "/tmp/zigtw/x86_64-windows-gnu-cc" is not able to compile a simple test program.
  ...
  ar: error: unknown option g
```

`link.txt` 里 CMake 走的是 Windows 专属路径：`ar qc objects.a @objects1.rsp`，再 `-Wl,--whole-archive ... --out-implib`。但我单独复现 `zig ar qc x.a @rsp` 是 **rc=0 正常**的，所以**成因我没有钉死**，不下结论。已排除的一项：`@响应文件` 语法本身 zig ar 是支持的。

影响面：**主程序不受影响**（Rust 侧 cargo-zigbuild 走的是另一套链接路径，windows-gnu 主程序已实测通过，见 §1 矩阵）；只有 windows 的 `llama-server` 需要在本机/CI 上编。

### 7.2 交叉必须带 `static-ssl`

主程序默认通过 `reqwest` 的 default-tls → native-tls 拿 OpenSSL。实测本机 mac 产物动态链的是 homebrew 路径：

```
/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib
/opt/homebrew/opt/openssl@3/lib/libcrypto.3.dylib
```

这个路径在 linux/windows 上不存在。`static-ssl = ['openssl/vendored']`（[crates/tabby/Cargo.toml:18](../../crates/tabby/Cargo.toml#L18)）把 OpenSSL 源码静态编进来——`openssl-src 300.3.0+3.3.0` **自带完整 OpenSSL 源码且已在本地 cargo 缓存**，所以不联网也能编。实测产物内含 `OpenSSL 3.3.0` 字符串且无 libssl 动态依赖，这条路是通的。

另注：`GLIBC=2.28` 这类版本钉是 cargo-zigbuild 的 `--target triple-<major>.<minor>` 语法。不钉时产物按 zig 默认的 glibc 需求生成，**在老发行版上可能跑不起来**；脚本留了这个口子但本轮**未实测**。

### 7.3 必须在 workspace 根执行，因为 sqlx 编译期宏

`ee/tabby-db` 大量用 `query_as!` / `query_scalar!` / `migrate!`，需要编译期拿到数据库结构。仓库没有 `.sqlx` 离线目录，靠的是两个**被 git 追踪**的文件：

```
.env                             -> DATABASE_URL=sqlite://ee/tabby-db/schema.sqlite
ee/tabby-db/schema.sqlite        -> 供宏校验的空 schema 库
```

sqlx 宏从**当前工作目录**读 `.env`。换了 cwd 就会在编译期炸，报错长得像数据库问题，实际是路径问题。

### 7.4 `llama-server` 必须显式传 `--target` 才能落在产物目录

`build.rs` 用 `omnicopy_to_output::copy_to_output` 搬二进制，其实现（`omnicopy_to_output-0.1.1/src/lib.rs` 第 121-125 行）是**判断 OUT_DIR 里是否含 `target/{triple}/{profile}` 这一段**来定位产物目录。不传 `--target` 时 OUT_DIR 形如 `target/debug/build/.../out`，没有 triple 段，于是 `target/` 下找不到 `llama-server`。

实测佐证：本机跑过 `--features cuda` 与 `--features binary` 两种构建后，`find target -name llama-server -not -path "*/build/*"` **返回空**——文件只在 `target/debug/build/llama-cpp-server-*/out/bin/` 里。官方 CI 的 `mv target/${{ matrix.target }}/release/llama-server` 之所以成立，正是因为它总是带 `--target`。

### 7.5 UI 是编译期嵌入的，产物可能带旧界面

[ee/tabby-webserver/src/routes/ui.rs:8](../../ee/tabby-webserver/src/routes/ui.rs#L8) 用 `rust-embed` 的 `#[folder = "./ui"]` 在编译期打包 `ee/tabby-webserver/ui`。本轮实测该目录停在 **8/12**，而 `ee/tabby-ui` 源码更新——即不重建 UI 就必然发布旧界面。脚本会检测并告警，`--refresh-ui` 才真正重建（`pnpm build` 后同步到 `ee/tabby-webserver/ui`，等价于 `make update-ui`）。

### 7.6 rustc 版本与 CI 不一致

官方 release.yml 钉 `RUST_TOOLCHAIN: 1.82.0`，而仓库**没有 `rust-toolchain` 文件**，所以本机一律用默认 toolchain（实测 1.99.0）。实测 `--no-default-features --features prod` 在 1.99.0 下能编过（1m49s），release 交叉路线也在 1.99.0 上跑通（见 §7.7 的对照表），但这意味着**本机产物与官方产物不是同一编译器输出**，做事故回溯时不要把两者混为一谈。

### 7.7 release + 旧 toolchain 会产出「加载不了」的 proc-macro dylib（本轮最主要的坑）

**症状**：交叉编译跑到 `sqlx` 时炸，报错分两层，第二层完全是第一层的连带：

```
error: .../target/release/deps/libsqlx_macros-251cbc96049ad6d9.dylib:
  dlopen(..., 0x0005): ... (mis-aligned LINKEDIT string pool, fileOffset=0x00645664)
error[E0432]: unresolved imports `self::encode::Encode`, ... `super::FromRow`
error: could not compile `sqlx` (lib) due to 2 previous errors
```

**它不是 feature 没开**。E0432 看着像 sqlx 的 backend/macros 特性组合错了，实际是 `sqlx` 在 `pub extern crate sqlx_macros` 处 dlopen 失败，宏整个不可用，于是宏门控的模块没生成。判据（实测）：同一 `target/release/deps` 里 39 个 proc-macro dylib **只有这 1 个**坏，其余 `dlopen` 全 OK。

**定位到字节级**：Mach-O 的 `LC_SYMTAB.stroff` 必须 8 字节对齐，macOS 27 的 dyld 会硬拒。坏的这个 `stroff=6575716`（%8==4），对照组全是 %8==0：

```bash
otool -l <dylib> | grep -A4 LC_SYMTAB | grep stroff
```

**归因（判别实验做的，不是推的）**：同一套命令、同一仓库、同一台机器，只换编译器就分叉——

| toolchain | profile | `sqlx-macros` 链接结果 |
|---|---|---|
| 1.94.0 | debug | OK（3 个 dylib 全部 dlopen 通过） |
| 1.94.0 | **release** | **mis-aligned，dlopen 失败**；删掉产物重编，新文件同样坏（确定性，非半途写坏） |
| 1.99.0（默认） | release | OK，`sqlx-macros → sqlx → sqlx-migrate-validate` 一路编过 |

所以这是 **1.94.0 的链接产物在本机 dyld 上不被接受**，与 llama.cpp、与 sqlx、与本仓库代码都无关。**结论：release 一律用默认 toolchain，不要 `TOOLCHAIN=1.94.0`**；旧 toolchain 只在「默认 toolchain 缺 cross std 且装不上、又只要 debug 产物」时才用。

> 附带一个取证教训：脚本失败时外层 `bash -c "... ; echo EXIT=$?"` 会让工具报「exit code 0」，真正的 101 只在 `EXIT=` 那行里。**cargo/cmake 的成败要看被包裹命令自己的退出码。**

### 7.8 windows 交叉要 `nasm`，而且是被本轮新加的 MCP 依赖拖进来的

`x86_64-pc-windows-gnu` 第一次跑失败，报的不是链接错误而是构建脚本 panic：

```
thread 'main' panicked at .../aws-lc-sys-0.41.0/builder/nasm_builder.rs:138:13:
  NASM command not found! Build cannot continue.
```

依赖链（`cargo tree -i aws-lc-sys --target x86_64-pc-windows-gnu` 实测）：

```
aws-lc-sys ← aws-lc-rs ← jsonwebtoken ← rust-mcp-sdk / rust-mcp-axum ← tabby-mcp-server ← tabby
```

三点值得记住：

1. **它是「谁引入的」问题，不是「怎么绕过」问题**。本仓库自己没声明 aws-lc，是 `crates/tabby-mcp-server` 的 `rust-mcp-*` 间接带上的（[crates/tabby-mcp-server/Cargo.toml:13](../../crates/tabby-mcp-server/Cargo.toml#L13)）；官方 release.yml 用 windows-latest 原生 runner，天然自带汇编器，所以这条边界在 CI 上不可见。
2. **`AWS_LC_SYS_NO_ASM=1` 在 release 下被明确拒绝**：`cmake_builder.rs:188` 直接 panic `AWS_LC_SYS_NO_ASM only allowed for debug builds!`。别把它当免安装 nasm 的后门。
3. linux 路线**不需要** nasm（同一依赖在 linux 上走 GAS/预生成对象），所以 `brew install nasm` 只为 windows 服务。装上后同一命令立刻通过，产物 `PE32+ executable (console) x86-64, for MS Windows`。

### 7.9 跨 arch 的 apple 目标必须 `static-ssl`；产物要过架构自检

给 `x86_64-apple-darwin`（Intel mac）编 tabby 时，`openssl-sys` 的构建脚本直接 panic：

```
thread 'main' panicked at .../openssl-sys-0.9.102/build/find_normal.rs:190:5:
  Could not find directory of OpenSSL installation, and this `-sys` crate cannot proceed
```

原因很物理：本机 `/opt/homebrew/opt/openssl@3` 只有 arm64 一份，openssl-sys 找到后也会因架构不匹配拒绝/失败。同架构目标（aarch64-apple-darwin）用系统 openssl 没问题，所以脚本规则是：**apple 目标里 arch 与宿主相同的按官方 CI 用系统 openssl，跨 arch 的自动加 `static-ssl`**（vendored OpenSSL 源码就地编一份 x86_64）。实测加完就通，产物 106MB x86_64 Mach-O。

配套加了一道**产物架构自检**（`assert_arch`，`file -b` 比对 target arch）再打包。动机是 `llama-cpp-server/build.rs` 编 Metal 版时不传 `CMAKE_OSX_ARCHITECTURES`，理论上存在「cargo 目标是 x86_64、cmake 按宿主出 arm64、脚本原样打包」的静默错架构风险。实测这个风险**没有发生**——cmake crate 会按 cargo 的 `TARGET` 自动定 arch，x86_64-apple 的 llama-server 出来就是真 x86_64 Mach-O（`otool -L` 含 Metal/MetalKit）——但自检保留：它把「二进制拷得出、目标机跑不了」这类静默事故变成打包前的硬失败，成本一行 `file`。

---

## 8. 交付与校验

```bash
file dist/tabby_x86_64-unknown-linux-gnu/tabby       # 期望 ELF, x86-64
otool -L dist/tabby_aarch64-apple-darwin/tabby | grep Metal   # mac 版应有 Metal
cat dist/tabby_*.tar.gz.sha256
```

目标机上的冒烟（不需要模型也能验运行栈）：

```bash
./tabby --version
./tabby serve --help
```

---

## 9. 磁盘占用

每多一个 target 就多一整套依赖产物。本轮实测：五个 target 加宿主 debug/release，`target/` 合计 **~46GB**（x86_64-gnu 9.3G、x86_64-musl 8.7G、aarch64-gnu 8.3G、宿主 debug 8.8G……），交叉 cmake 的中间产物另占 `.runbook-cache` **649MB**。只删单个 target：

```bash
du -sh target/*/ | sort -h | tail
rm -rf target/x86_64-unknown-linux-gnu          # 删整个 target 目录
cargo clean -p llama-cpp-server                 # 或只清 cmake 的 out/ 子树
rm -rf .runbook-cache                           # 交叉 llama 的 cmake 构建目录（可重建）
```

`dist/` 由脚本每次覆盖对应 target，可放心反复跑。
