# system

本仓在初始 API 合同（S1）历史上增加 System 主体库、客户端和共享启动辅助。
这些包都是 `no_std` 库；实际组合、Deployment 选择和 ELF 入口仍由消费者拥有。

## 目录

- `api/`：`system-api` 库与新增的 `api/tests/shape.rs` 结构判据。
- `server/`：`system-server`，接收一次取得的 Boot、显式声明集和已选择的 Deployment。
- `client/`：`system-client`，保留原有协议及调用行为。
- `support/`：`system-support`，共享启动账、机器解析、时序常量和日志；不依赖 server。
- `tests/identity-api/`：API 的 identity 合同测试，独立 Cargo 工作区。
- `tests/loader-api/`：API 的 loader 合同测试，独立 Cargo 工作区。
- `tests/` 中其余 27 个独立工作区：迁入的 server/client 宿主测试与拆分后的结构、RPC、watch 测试；归属见 `TEST-OWNERSHIP.md`。

`api` 的正常依赖只有公开组件 `env`、`wire`、`mold`，三者固定到已验收的 Git
修订。测试包只依赖 API 与这些公开组件；API 不依赖 unit、server 或 client。

## 工具链

`rust-toolchain.toml` 固定为 `nightly-2026-09-04`，与来源工程一致。

## 构建与测试

```sh
cargo check --manifest-path api/Cargo.toml --target x86_64-unknown-linux-gnu --locked
cargo check --manifest-path api/Cargo.toml --target riscv64gc-unknown-none-elf --locked
cargo test --manifest-path api/Cargo.toml --tests --target x86_64-unknown-linux-gnu --locked
cargo test --manifest-path tests/identity-api/Cargo.toml --target x86_64-unknown-linux-gnu --locked
cargo test --manifest-path tests/loader-api/Cargo.toml --target x86_64-unknown-linux-gnu --locked
cargo check -p system-server --target riscv64gc-unknown-none-elf --locked
cargo test -p system-server --lib --target x86_64-unknown-linux-gnu --locked
cargo check -p system-server --features test-support --target riscv64gc-unknown-none-elf --locked
cargo test -p system-server --lib --features test-support --target x86_64-unknown-linux-gnu --locked
```

缓存就绪后，同一组命令加 `--offline` 复验。实际测试为 shape 3 个、identity 9
个、loader 6 个，共 18 个；server 原有内联宿主测试另有 23 个。上述测试不需要 QEMU。
另有 27 个迁入/拆分工作区共 317 项宿主测试。`RELEASE-CHECKPOINT.md` 区分这些结果
与尚待消费端运行的整机、故障和最终回收验收。

server/client 固定使用旧发布 `9e5d63ca536a176583cf1b0054b4ecf7ddd88257`
的 `system-api`，与 `unit` 的类型来源一致；本仓 `api/` 成员继续独立测试其原始文件。
同仓 client/support 使用 path，跨仓公共依赖使用完整 GitHub rev。

根工作区与两个测试包各自跟踪一份 `Cargo.lock`。根锁由来源已跟踪的根锁经实际
Cargo 解析裁剪生成；两个测试锁来自来源工作树已验收的生成锁并保持字节不变。
三者都是生成锁，在本仓作为自有锁纳入跟踪，不是上游 API 自带的锁。

## 边界

当前 provider 库检查点尚未完成 Source 消费者切换。Source 宿主测试归属、私有
状态探针的测试支持边界、整机与最终回收验收仍待实施，不能称阶段 3/4 完成。
`MIGRATION-MAP.json` 记录来源与逐文件哈希；历史 API 的署名与许可说明保留在
`PROVENANCE.md`。
