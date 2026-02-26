//! 使用用户态中断的通知机制
use crate::interface::NotificationIf;

/// 使用用户态中断的通知机制（未完成）
pub struct UIntrNotification;

impl NotificationIf for UIntrNotification {
    fn new_id() -> Option<u64> {
        todo!()
    }

    async fn wait_on(id: u64) {
        todo!()
    }

    unsafe fn release_id(id: u64) {
        todo!()
    }

    fn notify(process: u64, id: u64) {
        todo!()
    }
}
