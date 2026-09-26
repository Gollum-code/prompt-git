//! prompt-git：把提示词当代码管理。
//! git 存储 + 结构化 diff + 每次变更自动跑评测门禁。

pub mod compare;
pub mod diff;
pub mod eval;
pub mod store;
pub mod template;
pub mod test;