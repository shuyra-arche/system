# system

本仓是 System 公共 API 的独立发布单元：`no_std` 的 `system-api` 库，以及该 API
直接拥有的两个测试包。它承载 System 的初始 API 合同（S1），不包含 System
runtime、client、unit、Kernel 等后续阶段。

## 目录

- `api/`：`system-api` 库与新增的 `api/tests/shape.rs` 结构判据。
- `tests/identity-api/`：API 的 identity 合同测试，独立 Cargo 工作区。
- `tests/loader-api/`：API 的 loader 合同测试，独立 Cargo 工作区。

`api` 的正常依赖只有公开组件 `env`、`wire`、`mold`，三者固定到已验收的 Git
修订。测试包只依赖 API 与这些公开组件；API 不依赖 unit、server 或 client。

## 工具链

`rust-toolchain.toml` 固定为 `nightly-2026-09-04`，与来源工程一致。

## 构建与测试

```sh
cargo check -p system-api --target x86_64-unknown-linux-gnu --locked
cargo check -p system-api --target riscv64gc-unknown-none-elf --locked
cargo test -p system-api --tests --target x86_64-unknown-linux-gnu --locked
cargo test --manifest-path tests/identity-api/Cargo.toml --target x86_64-unknown-linux-gnu --locked
cargo test --manifest-path tests/loader-api/Cargo.toml --target x86_64-unknown-linux-gnu --locked
```

缓存就绪后，同一组命令加 `--offline` 复验。实际测试为 shape 3 个、identity 9
个、loader 6 个，共 18 个。测试不需要 QEMU。

根工作区与两个测试包各自跟踪一份 `Cargo.lock`。根锁由来源已跟踪的根锁经实际
Cargo 解析裁剪生成；两个测试锁来自来源工作树已验收的生成锁并保持字节不变。
三者都是生成锁，在本仓作为自有锁纳入跟踪，不是上游 API 自带的锁。

## 边界

本仓只发布 API 初始合同。未来的 unit 与 System 运行体将消费同一个 Git S1；
不能据此认为 System runtime 已经迁入本仓或迁移完成。来源工程在提取后仍保留
旧结构判据，消费者阶段才负责移除旧半闭环。
