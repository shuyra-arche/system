# SDK 依赖验证

当前候选已合入真实默认分支 `codex/source-snapshot` 修订
`68197a0b37096f61da606cddf8fa27991c6f0d40`。它拥有 `system` 可执行文件、
`server/unit.toml`、动态 Unit 启动及原生生命周期事件。相对该默认分支，全部
Rust、Unit、构建脚本和测试源码保持不变；五项运行资源声明、Unit SDK
`ad50b803c3bd5cd2ef90010d80badf23551e88d5` 和动态测试工作区排除项也完整保留。

早先 SDK 提交 `8eea7c9` 基于历史 `codex/system-runtime-extraction@5a792ff`，
当时只有四个 `no_std` 库，尚无独立 ELF。本节当前默认分支验证与那次库检查
分开，不把历史分支当作默认落点。

## 依赖选择

Server、Client 和 Support 的 Execution 依赖统一固定为
`ea49192e5df8bf744e083ced23ff0908b5bab3a3`。该包复用 Kernel Client 的机制，
保留既有程序入口、参数捕获、panic 和链接脚本接口。

| 包 | 公开修订 |
| --- | --- |
| Env | `f54f9a1ab3f0df3631fb80ff2bf5d0cfaecbe927` |
| Env initrd reader | `f54f9a1ab3f0df3631fb80ff2bf5d0cfaecbe927` |
| Wire | `4b7619fda4bbd202e9a5c952872a84c2145c681e` |
| Mold | `1addd302789e6a1c0dea6a7adf27e975a05d5ada` |
| Execution | `ea49192e5df8bf744e083ced23ff0908b5bab3a3` |
| Kernel API 和 Client | `90d43dbf3ee43fd146f487718eb39e88f2507c05` |

根 `Cargo.toml` 的三条 `[replace]` 精确匹配旧 Env、Wire 和 Mold 的 Git URL、
rev、包名及版本，选择表中的公开修订。这个根组合选择同时覆盖历史间接依赖，
不需要本地路径覆盖、URL 别名或复制 ABI 定义。生成锁中的旧包及 `replace`
记录用于描述这一选择，不代表旧实现仍在活动依赖图中。所有 registry 包的版本
和校验值与真实默认分支 `68197a0` 相同。

Server 和 Support 两处 `initrd` 直接选择表中的纯 API reader。Server 使用
同工作区 `../client`，避免再次取得历史 Git Client 及其旧 Execution；该 Client
的 Rust 源码与默认分支相同。Support 保留已有工作区依赖关系。

`[replace]` 是 Cargo 已弃用但当前工具链仍支持的机制。本检查点以它保留原始
`cargo --locked` 独立构建入口；同源 Git `[patch]` 无法选择另一个修订。选择器只
适用于这些明确的旧 pin；依赖修订变化时须重新核对锁与实际依赖图，待依赖链
统一后应移除这些选择器。Execution 直接改 pin，保留 `links = "execution"`
以及 `DEP_EXECUTION_LINKER_SCRIPT`，没有对它使用替换或更改链接元数据。

Cargo 只读取组合根的 `[replace]`。其他仓库依赖这些 System 库时，不继承本根
选择；消费者仍负责统一其 Env/Wire/Mold 来源。Server 和 Client 保留公开
`system-api@9e5d63ca536a176583cf1b0054b4ecf7ddd88257`，与既有 Unit 类型来源
一致，同仓 `api/` 成员保留其独立结构测试。该选择未改变 Runtime 退出语义。

## 本轮验证

使用 `nightly-2026-10-09`，Cargo、rustc 和 rustdoc 均选择该工具链。对正式
生成锁执行以下命令，没有外部 Cargo 配置或开发源码覆盖：

```sh
cargo check --locked --offline -p system-server --bin system --target riscv64gc-unknown-none-elf
cargo build --locked --offline --release -p system-server --bin system --target riscv64gc-unknown-none-elf
cargo test --locked --offline -p system-api --tests --target x86_64-unknown-linux-gnu
cargo test --locked --offline -p system-server --lib --target x86_64-unknown-linux-gnu
cargo test --locked --offline --manifest-path tests/dynamic-units/Cargo.toml --target x86_64-unknown-linux-gnu
cargo test --locked --offline --manifest-path tests/instance-events/Cargo.toml --target x86_64-unknown-linux-gnu
cargo tree --locked --offline -p system-server --target riscv64gc-unknown-none-elf \
  --edges normal,build --prefix none --format '{p}'
cargo tree --locked --offline -p env-initrd --target riscv64gc-unknown-none-elf \
  --edges normal,build --prefix none --format '{p}'
```

独立 `system` RV64 检查和 release ELF 链接通过，生成 1,759,800 字节的静态
RISC-V 可执行文件，无未解析符号，SHA-256 为
`715d14960bd1ecd92f3d53eb1012337054eddbf2c5aac1c30553543fd246982e`。
API shape 3 项、Server 宿主断言 23 项、动态 Unit 2 项和实例事件 3 项通过，
后两个工作区的空 doctest 集也通过。

实际 Server normal/build 图只有一个 Kernel API 和 Client，均来自 `90d43db`；
Execution 只有 `ea49192` 一个来源。initrd reader 的活动图只有同一纯 API，
不含 Client。两个独立动态测试工作区保留原始锁和旧 reader pin，用于验证保留
的源码与生命周期判据；未把它们误记为生产 reader 的来源迁移验证。

既有未使用项警告仍保留。旧场景的 `test-support` feature 尚未跟随默认分支的
动态 Unit 结构迁移；其 RV64 检查仍失败，不能把历史 `5a792ff` 上的通过结果
外推到当前分支。在隔离的 `68197a0` 基线上用同一工具链、正式锁和同一
`cargo check --locked --offline -p system-server --features test-support
--target riscv64gc-unknown-none-elf` 复现 43 项编译错误；当前分支的错误码、
消息及顺序完全一致，涉及旧 `Declarations`、`File.elf` 和 Unit 类型形状。
本轮没有修改那些场景或用其以前的 317 项结果计数。09-04
工具链、QEMU、故障恢复和最终资源回收不在本检查点的验收声明内；这些结果由
各消费者的组合验证负责。
