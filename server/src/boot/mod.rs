//! 清单（装了哪些程序）与配对块（有哪些门闩）。
//! 这是**这台机器的事实**，不是协议：它读的是启动参数（env::ledger::args），答的还是
//! "谁被装进来了"这件事。
//! 内核只把这两区**只读借映**进**引导镜像那一域**，故读者是每一个引导镜像：
//! `system`，以及测具那一档的 `again` / `rig` / `load` / `group`（`src/harness/bench/`）。

pub use system_support::boot::{Accounts, Catalog};
pub use crate::system::app::bootstrap::{Boot, Fail, take};
