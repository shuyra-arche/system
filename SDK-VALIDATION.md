# SDK 依赖验证

本检查点基于公开的 `codex/system-runtime-extraction` 修订
`5a792ff2d39901a970925b1a6d63206adf6c16d8`。该版本提供 API、Server、Client 和
Support 四个 `no_std` 库，没有独立二进制或 ELF 入口。所有库、启动辅助、API
和测试源码保持不变；未引入当前其他 System 分支的行为。

## 依赖选择

Server、Client 和 Support 的 Execution 依赖统一固定为
`ea49192e5df8bf744e083ced23ff0908b5bab3a3`。该包复用 Kernel Client 的机制，
保留既有程序入口、参数捕获、panic 和链接脚本接口。

| 包 | 公开修订 |
| --- | --- |
| Env | `f54f9a1ab3f0df3631fb80ff2bf5d0cfaecbe927` |
| Wire | `4b7619fda4bbd202e9a5c952872a84c2145c681e` |
| Mold | `1addd302789e6a1c0dea6a7adf27e975a05d5ada` |
| Execution | `ea49192e5df8bf744e083ced23ff0908b5bab3a3` |
| Kernel API 和 Client | `90d43dbf3ee43fd146f487718eb39e88f2507c05` |

根 `Cargo.toml` 的三条 `[replace]` 精确匹配旧 Env、Wire 和 Mold 的 Git URL、
rev、包名及版本，选择表中的公开修订。这个根组合选择同时覆盖历史间接依赖，
不需要本地路径覆盖、URL 别名或复制 ABI 定义。生成锁中的旧包及 `replace`
记录用于描述这一选择，不代表旧实现仍在活动依赖图中。所有 registry 包的版本
和校验值与基线相同。

`[replace]` 是 Cargo 已弃用但当前工具链仍支持的机制。本检查点以它保留原始
`cargo --locked` 库构建入口；同源 Git `[patch]` 无法选择另一个修订。选择器只
适用于这些明确的旧 pin；依赖修订变化时须重新核对锁与实际依赖图，待依赖链
统一后应移除这些选择器。Execution 直接改 pin，保留 `links = "execution"`
以及 `DEP_EXECUTION_LINKER_SCRIPT`，没有对它使用替换或更改链接元数据。

Cargo 只读取组合根的 `[replace]`。其他仓库依赖这些 System 库时，不继承本根
选择；消费者仍负责统一其 Env/Wire/Mold 来源。Server 和 Client 保留公开
`system-api@9e5d63ca536a176583cf1b0054b4ecf7ddd88257`，与既有 Unit 类型来源
一致，同仓 `api/` 成员保留其独立结构测试。

## 本轮验证

使用 `nightly-2026-10-09`，对正式生成锁执行以下命令，没有外部 Cargo 配置或
开发源码覆盖：

```sh
cargo check --locked --offline --workspace --target riscv64gc-unknown-none-elf
cargo build --locked --offline --workspace --release --target riscv64gc-unknown-none-elf
cargo test --locked --offline -p system-api --tests --target x86_64-unknown-linux-gnu
cargo test --locked --offline -p system-server --lib --target x86_64-unknown-linux-gnu
cargo check --locked --offline -p system-server --features test-support --target riscv64gc-unknown-none-elf
cargo test --locked --offline -p system-server --lib --features test-support --target x86_64-unknown-linux-gnu
cargo tree --locked --offline --workspace --target riscv64gc-unknown-none-elf \
  --edges normal,build --prefix none --format '{p}'
```

默认 RV64 检查与 release 库构建通过；API 原有 shape 断言 3 项、Server 原有
宿主断言 23 项通过。`test-support` 的 RV64 检查与同组 23 项宿主断言也通过。
实际 normal/build 依赖图只有一个 Kernel API 和 Client，均来自 `90d43db`；
Execution 只有 `ea49192` 一个来源。没有旧 Execution 入口或两组链接元数据。

既有未使用项警告仍保留。本轮未修改或重跑 `tests/` 下独立历史工作区，不把
它们以前的 317 项结果算作本次通过。09-04 工具链、消费者 ELF、QEMU、故障恢复
和最终资源回收不在本检查点的验收声明内；这些结果由各消费者的组合验证负责。
