//! 使用统一的接口封装信号、用户态中断等通知机制，使其可用于IPC的通知中。

#![no_std]
#![deny(missing_docs)]
extern crate alloc;

pub mod interface;
#[cfg(feature = "signal")]
pub mod signal;
pub mod uintr;
