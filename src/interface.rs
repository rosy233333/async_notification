use core::ops::Not;

#[cfg(feature = "signal")]
use crate::signal::SignalNotification;

use crate::uintr::UIntrNotification;

pub trait NotificationIf {
    /// 在本进程申请一个新的通知源（例如中断向量或信号编号）
    ///
    /// id的高8位需被保留，从而区分不同类型的通知源
    fn new_id() -> Option<u64>;
    /// 在一个通知源上等待
    async fn wait_on(id: u64);
    /// 释放通知源
    ///
    /// SAFETY:
    ///
    /// - 在调用`release_id`时，不能有相应id上的`wait_on`还在执行中。
    /// - 在调用`release_id`之后、使用`new_id`分配到相同id之前，不能在该id上调用`wait_on`
    unsafe fn release_id(id: u64);
    /// 向另一进程的、相应ID的本类型通知源发送通知，唤醒在其上`wait_on`的协程
    fn notify(process: u64, id: u64);
}

const SIGNAL_HIGH8: u64 = 0x01 << 56;
const UINTR_HIGH8: u64 = 0x02 << 56;

pub struct Notification;

impl NotificationIf for Notification {
    /// 返回`None`
    ///
    /// 不应使用该函数，而应使用具体通知源类型的新建函数。
    fn new_id() -> Option<u64> {
        None
    }

    async fn wait_on(id: u64) {
        let high8 = id & 0xFF00_0000_0000_0000;
        let id_inner = id & 0x00FF_FFFF_FFFF_FFFF;
        match high8 {
            #[cfg(feature = "signal")]
            SIGNAL_HIGH8 => SignalNotification::wait_on(id_inner).await,
            UINTR_HIGH8 => UIntrNotification::wait_on(id_inner).await,
            _ => panic!("wait_on: Unknown notification type with id: 0x{:016x}", id),
        }
    }

    unsafe fn release_id(id: u64) {
        let high8 = id & 0xFF00_0000_0000_0000;
        let id_inner = id & 0x00FF_FFFF_FFFF_FFFF;
        match high8 {
            #[cfg(feature = "signal")]
            SIGNAL_HIGH8 => SignalNotification::release_id(id_inner),
            UINTR_HIGH8 => UIntrNotification::release_id(id_inner),
            _ => panic!(
                "release_id: Unknown notification type with id: 0x{:016x}",
                id
            ),
        }
    }

    fn notify(process: u64, id: u64) {
        let high8 = id & 0xFF00_0000_0000_0000;
        let id_inner = id & 0x00FF_FFFF_FFFF_FFFF;
        match high8 {
            #[cfg(feature = "signal")]
            SIGNAL_HIGH8 => SignalNotification::notify(process, id_inner),
            UINTR_HIGH8 => UIntrNotification::notify(process, id_inner),
            _ => panic!("notify: Unknown notification type with id: 0x{:016x}", id),
        }
    }
}

impl Notification {
    /// 该函数需要在tokio运行时内部调用，因为其会同时开始信号的接收。
    #[cfg(feature = "signal")]
    pub fn new_id_signal() -> Option<u64> {
        SignalNotification::new_id().map(|id| (id & 0x00FF_FFFF_FFFF_FFFF) | SIGNAL_HIGH8)
    }
}
