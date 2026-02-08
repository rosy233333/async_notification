//! 必须配合tokio运行时

use crate::interface::NotificationIf;
use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use futures::stream::{StreamExt, iter};
use lazyinit::LazyInit;
use signal_hook_tokio::{Signals, SignalsInfo};

pub struct SignalNotification;

struct SignalsInfoWrapper {
    used: AtomicBool,
    info: UnsafeCell<Option<SignalsInfo>>,
}

unsafe impl Sync for SignalsInfoWrapper {}

/// 用于本模块的信号数量
static SIG_NUM: LazyInit<usize> = LazyInit::new();
// const USED_CAPABILITY: usize = 65;

/// 用于本模块的信号
///
/// Linux下的[SIGRTMIN, SIGRTMAX]（[34, 64]）
static SIGNALS: LazyInit<Vec<u32>> = LazyInit::new();

/// 每个信号的占用情况及接收情况。
///
/// - Vec的index对应信号编号
/// - Some(SignalsInfo)代表该信号目前被占用
/// - None代表该信号目前未被占用
static USED: LazyInit<Vec<SignalsInfoWrapper>> = LazyInit::new();

/// 下一个分配的信号在`SIGNALS`中的index；
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// 模块是否初始化
static IS_INIT: AtomicBool = AtomicBool::new(false);

impl NotificationIf for SignalNotification {
    /// id即为分配的信号编号，取值区间[34, 64]
    fn new_id() -> Option<u64> {
        if !IS_INIT.load(Ordering::Acquire) {
            Self::init();
        }

        // 求余操作的除数
        let mod_: usize = SIG_NUM.next_power_of_two();
        // 代替求余的与操作的mask
        let mask: usize = mod_ - 1;

        let mut curr_next = NEXT.fetch_add(1, Ordering::AcqRel);
        for _ in 0..mod_ {
            let index = curr_next & mask;
            if index >= *SIG_NUM {
                continue;
            }
            if USED[SIGNALS[index] as usize]
                .used
                .swap(true, Ordering::AcqRel)
            {
                // 该信号已被占用
                curr_next = NEXT.fetch_add(1, Ordering::AcqRel);
            } else {
                // 该信号未被占用
                unsafe {
                    (&mut *(USED[SIGNALS[index] as usize].info.get()))
                        .replace(Signals::new([SIGNALS[index] as i32]).unwrap())
                };
                return Some(SIGNALS[index] as u64);
            }
        }
        None
    }

    async fn wait_on(id: u64) {
        if !IS_INIT.load(Ordering::Acquire) {
            Self::init();
        }

        assert!(SIGNALS.contains(&(id as u32)));
        unsafe { &mut *(USED[id as usize].info.get()) }
            .as_mut()
            .unwrap()
            .next()
            .await;
    }

    unsafe fn release_id(id: u64) {
        if !IS_INIT.load(Ordering::Acquire) {
            Self::init();
        }

        assert!(SIGNALS.contains(&(id as u32)));
        unsafe { &mut *(USED[id as usize].info.get()) }.take();
        let res = USED[id as usize].used.swap(false, Ordering::AcqRel);
        assert!(res); // 释放某id前，其必须已被占用
    }

    fn notify(process: u64, id: u64) {
        let res = unsafe { libc::kill(process as libc::pid_t, id as libc::c_int) };
        assert!(res == 0);
    }
}

impl SignalNotification {
    fn init() {
        assert!(!IS_INIT.swap(true, Ordering::AcqRel));
        #[cfg(feature = "log")]
        log::info!("SignalNotification init");
        let mut signum: usize = 0;
        let mut signals: Vec<u32> = Vec::new();
        for i in libc::SIGRTMIN()..=libc::SIGRTMAX() {
            if ![
                0x3f, // sender panic，libc::kill返回非0
                0x40, // sender panic，libc::kill返回非0
            ]
            .contains(&i)
            {
                signals.push(i as u32);
                signum += 1;
            }
        }
        SIG_NUM.init_once(signum);

        #[cfg(feature = "log")]
        log::info!("SIGNALS: {:?}", signals);
        SIGNALS.init_once(signals);
        let mut used: Vec<SignalsInfoWrapper> = Vec::new();
        for _ in 0..=libc::SIGRTMAX() {
            used.push(SignalsInfoWrapper {
                used: AtomicBool::new(false),
                info: UnsafeCell::new(None),
            });
        }
        USED.init_once(used);
    }
}

mod tests {
    use core::{ptr, task, time};

    // use super::*;
    use crate::interface::{Notification, NotificationIf};
    use alloc::vec::Vec;

    extern crate std;

    #[test]
    fn test_signal_manual() {
        use tokio::task::JoinHandle;

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let mut ids: Vec<u64> = Vec::new();
                while let Some(id) = Notification::new_id_signal() {
                    ids.push(id);
                }

                let mut handles: Vec<JoinHandle<()>> = Vec::new();
                for id in ids {
                    handles.push(tokio::spawn(async move {
                        std::println!("before block on id {:#018x}", id);
                        Notification::wait_on(id).await;
                        std::println!("after block on id {:#018x}", id);
                    }));
                }

                for handle in handles {
                    handle.await.unwrap();
                }
            });
    }

    #[test]
    fn test_signal_wakeup() {
        use tokio::task::JoinHandle;
        const SIGNAL_HIGH8: u64 = 0x01 << 56;

        let mut ids: Vec<u64> = Vec::new();
        // while let Some(id) = Notification::new_id_signal() {
        //     ids.push(id);
        // }
        for i in libc::SIGRTMIN()..=libc::SIGRTMAX() {
            if ![
                0x3f, // sender panic，libc::kill返回非0
                0x40, // sender panic，libc::kill返回非0
            ]
            .contains(&i)
            {
                ids.push((i as u64) | SIGNAL_HIGH8);
            }
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
                        let mut actual_ids: Vec<u64> = Vec::new();
                        while let Some(id) = Notification::new_id_signal() {
                            actual_ids.push(id);
                        }
                        ids_c.iter().for_each(|id| {
                            assert!(actual_ids.contains(id));
                        });
                        actual_ids.iter().for_each(|id| {
                            assert!(ids_c.contains(id));
                        });
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
