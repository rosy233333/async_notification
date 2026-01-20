use crate::interface::NotificationIf;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use futures::stream::StreamExt;
use signal_hook_tokio::Signals;

/// 必须配合tokio运行时
pub struct SignalNotification;

/// 用于本模块的信号数量
const SIG_NUM: usize = 31;

/// 用于本模块的信号
///
/// Linux下的[SIGRTMIN, SIGRTMAX]（[34, 64]）
static SIGNALS: [u32; SIG_NUM] = [
    34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57,
    58, 59, 60, 61, 62, 63, 64,
];

/// 每个信号是否被占用
static USED: [AtomicBool; SIG_NUM] = [const { AtomicBool::new(false) }; SIG_NUM];

/// 下一个分配的信号在`SIGNALS`中的index；
static NEXT: AtomicUsize = AtomicUsize::new(0);

impl NotificationIf for SignalNotification {
    /// id即为分配的信号编号，取值区间[34, 64]
    fn new_id() -> Option<u64> {
        /// 求余操作的除数
        const MOD: usize = SIG_NUM.next_power_of_two();
        /// 代替求余的与操作的mask
        const MASK: usize = MOD - 1;

        let mut curr_next = NEXT.fetch_add(1, Ordering::AcqRel);
        for _ in 0..MOD {
            let index = curr_next & MASK;
            if index >= SIG_NUM {
                continue;
            }
            if USED[index].swap(true, Ordering::AcqRel) {
                // 该信号已被占用
                curr_next = NEXT.fetch_add(1, Ordering::AcqRel);
            } else {
                // 该信号未被占用
                return Some(SIGNALS[index] as u64);
            }
        }
        None
    }

    async fn wait_on(id: u64) {
        assert!(SIGNALS.contains(&(id as u32)));
        let mut signals = Signals::new([id as i32]).unwrap();
        signals.next().await;
    }

    fn release_id(id: u64) {
        assert!(SIGNALS.contains(&(id as u32)));
        let res = USED[id as usize].swap(false, Ordering::AcqRel);
        assert!(res); // 释放某id前，其必须已被占用
    }

    fn notify(process: u64, id: u64) {
        let res = unsafe { libc::kill(process as libc::pid_t, id as libc::c_int) };
        assert!(res == 0);
    }
}

mod tests {
    use core::{ptr, task, time};

    // use super::*;
    use crate::interface::{Notification, NotificationIf};
    use alloc::vec::Vec;

    extern crate std;

    #[test]
    fn test_signal_wakeup() {
        use tokio::task::JoinHandle;

        let mut ids: Vec<u64> = Vec::new();
        while let Some(id) = Notification::new_id_signal() {
            ids.push(id);
        }

        for id in &ids {
            std::println!("{:#018x}", *id);
        }

        let ids_c = ids.clone();

        match unsafe { libc::fork() } {
            0 => {
                // child
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async move {
                        let mut handles: Vec<JoinHandle<()>> = Vec::new();
                        for id in ids_c {
                            handles.push(tokio::spawn(async move {
                                std::println!("before block on id {:#018x}", id);
                                Notification::wait_on(id).await;
                                std::println!("after block on id {:#018x}", id);
                            }));
                        }

                        for handle in handles {
                            handle.await;
                        }
                    });
            }
            -1 => panic!("Fork failed!"),
            child => {
                // parent
                std::thread::sleep(time::Duration::from_secs(1));
                for id in &ids {
                    Notification::notify(child as u64, *id);
                }

                unsafe {
                    libc::waitpid(child as i32, ptr::null_mut(), 0);
                }
            }
        }
    }
}
