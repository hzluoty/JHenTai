//! UI 事件泵：把跨线程标志（目前是封面、详情缩略图、返回键）在 UI 线程上排干。
//!
//! 定时器回调只能碰 Send 的东西（`AtomicBool`/`Arc<Mutex<..>>`），
//! 真正的 Slint 模型修改都发生在 UI 线程。
//!
//! 例外（JNI 事件区）：Java 主线程写入的原子量都放在 `nav.rs`（`BACK_REQUESTED`、
//! `VOLUME_KEY_STEP`、`VOLUME_CONSUMER`、`NAV_DEPTH`），同样由这里的泵消费。
//! 新增「tokio → UI」事件进 `UiEvents`；新增「Java → UI」事件进 `nav.rs`，不要另开通道。

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use slint::Timer;

/// 跨线程置位的 UI 事件。加新事件时先在这里加标志，再在 `pump` 里排干。
#[derive(Debug, Default)]
pub struct UiEvents {
    pub covers: AtomicBool,
    pub details_thumbs: AtomicBool,
    pub reader: AtomicBool,
    pub more_thumbs: AtomicBool,
    pub downloads: AtomicBool,
    pub account: AtomicBool,
    pub tag_translation: AtomicBool,
}

impl UiEvents {
    pub fn request_covers(&self) {
        self.covers.store(true, Ordering::Relaxed);
    }

    pub fn request_details_thumbs(&self) {
        self.details_thumbs.store(true, Ordering::Relaxed);
    }

    pub fn request_tag_translation(&self) {
        self.tag_translation.store(true, Ordering::Relaxed);
    }

    pub fn take_tag_translation(&self) -> bool {
        self.tag_translation.swap(false, Ordering::Relaxed)
    }

    pub fn request_account(&self) {
        self.account.store(true, Ordering::Relaxed);
    }

    pub fn take_account(&self) -> bool {
        self.account.swap(false, Ordering::Relaxed)
    }

    pub fn request_downloads(&self) {
        self.downloads.store(true, Ordering::Relaxed);
    }

    pub fn take_downloads(&self) -> bool {
        self.downloads.swap(false, Ordering::Relaxed)
    }

    pub fn request_more_thumbs(&self) {
        self.more_thumbs.store(true, Ordering::Relaxed);
    }

    pub fn take_more_thumbs(&self) -> bool {
        self.more_thumbs.swap(false, Ordering::Relaxed)
    }

    pub fn request_reader(&self) {
        self.reader.store(true, Ordering::Relaxed);
    }

    pub fn take_reader(&self) -> bool {
        self.reader.swap(false, Ordering::Relaxed)
    }

    pub fn take_covers(&self) -> bool {
        self.covers.swap(false, Ordering::Relaxed)
    }

    pub fn take_details_thumbs(&self) -> bool {
        self.details_thumbs.swap(false, Ordering::Relaxed)
    }
}

/// 启动泵：UI 线程上每 `interval` 排干一次。
pub fn start(timer: &Timer, controller: &Rc<crate::AppController>, interval: Duration) {
    let weak = Rc::downgrade(controller);
    timer.start(slint::TimerMode::Repeated, interval, move || {
        let Some(this) = weak.upgrade() else {
            return;
        };
        if this.ui_events.take_covers() {
            this.load_covers();
        }
        if this.ui_events.take_details_thumbs() {
            this.load_detail_thumbnails();
        }
        if this.ui_events.take_reader() {
            this.drain_reader_pending();
        }
        if this.ui_events.take_more_thumbs() {
            this.fetch_more_thumbnails();
        }
        if this.ui_events.take_downloads() {
            this.refresh_downloads_view();
        }
        if this.ui_events.take_account() {
            this.apply_account_to_ui();
        }
        if this.ui_events.take_tag_translation() {
            this.load_tag_translation();
        }
        // 返回键：Java 主线程只置标志，出栈必须在 UI 线程
        if crate::nav::BACK_REQUESTED.swap(false, Ordering::Relaxed) {
            this.go_back();
        }
        // 自动翻页（仅阅读页）：间隔来自 ReadSetting.autoModeInterval
        if this.auto_turn_enabled.get() && matches!(this.nav.borrow().current(), crate::nav::Page::Read { .. }) {
            let interval = std::time::Duration::from_secs_f64(
                this.settings.borrow().read.auto_mode_interval.max(0.5),
            );
            let due = {
                let mut last = this.auto_turn.lock().unwrap();
                match *last {
                    Some(t) if t.elapsed() < interval => false,
                    _ => {
                        *last = Some(std::time::Instant::now());
                        true
                    }
                }
            };
            // 到最后一页就停，避免空转并反复写阅读进度
            let at_last = {
                let state = this.reader.lock().unwrap();
                state.index + 1 >= state.len()
            };
            if due && !at_last {
                this.reader_step(1);
            }
        }

        // 音量键翻页（累计多步时逐张翻）。`matches!` 是防御：用户可能在 JNI 置位后
        // 立刻退出阅读页，此时页栈已经不是 Read，别把翻页作用到别的页面。
        let step = crate::nav::take_volume_step();
        if step != 0 && matches!(this.nav.borrow().current(), crate::nav::Page::Read { .. }) {
            for _ in 0..step.unsigned_abs() {
                this.reader_step(if step > 0 { 1 } else { -1 });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_one_shot() {
        let events = UiEvents::default();
        assert!(!events.take_covers());
        events.request_covers();
        assert!(events.take_covers());
        assert!(!events.take_covers(), "取过一次就清掉，避免每帧重复加载");

        events.request_details_thumbs();
        assert!(events.take_details_thumbs());
        assert!(!events.take_details_thumbs());

        events.request_reader();
        assert!(events.take_reader());
        assert!(!events.take_reader());

        events.request_more_thumbs();
        assert!(events.take_more_thumbs());
        assert!(!events.take_more_thumbs());

        events.request_downloads();
        assert!(events.take_downloads());
        assert!(!events.take_downloads());

        events.request_account();
        assert!(events.take_account());
        assert!(!events.take_account());

        events.request_tag_translation();
        assert!(events.take_tag_translation());
        assert!(!events.take_tag_translation());
    }
}
