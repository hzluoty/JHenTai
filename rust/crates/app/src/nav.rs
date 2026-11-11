//! 页栈导航，替代 GetX 的 57 条路由（计划 §4.3）。

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    GalleryList,
    Details {
        gid: i64,
        token: String,
    },
    Read {
        gid: i64,
    },
    Downloads,
    Settings,
}

/// 页栈深度与「返回键被按下」的跨线程标志。
///
/// Java 主线程的返回键回调不能碰 `Rc`，所以用原子量沟通：
/// JNI 侧读 `NAV_DEPTH` 决定是否消费按键，UI 定时器读 `BACK_REQUESTED` 执行出栈。
pub static NAV_DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
pub static BACK_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 阅读页是否消费音量键（由阅读器进出 + 设置开关维护）
pub static VOLUME_CONSUMER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 音量键翻页的累计步数：+n 表示向后翻 n 张，-n 表示向前（0 = 无请求）。
///
/// 用 `fetch_add` 累计：按住音量键时系统重发事件比 UI 泵（120ms）快，
/// `swap` 语义会丢掉中间的按键。
pub static VOLUME_KEY_STEP: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// 取出并清零累计步数
pub fn take_volume_step() -> i32 {
    VOLUME_KEY_STEP.swap(0, std::sync::atomic::Ordering::Relaxed)
}

/// 线性页栈。`back()` 出栈；栈底永远是 GalleryList。
#[derive(Debug, Default)]
pub struct Navigator {
    stack: Vec<Page>,
}

impl Navigator {
    pub fn new() -> Self {
        Self { stack: vec![Page::GalleryList] }
    }

    pub fn current(&self) -> &Page {
        self.stack.last().unwrap_or(&Page::GalleryList)
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// 压栈；重复压同一页（同 gid/token）不产生新层
    pub fn push(&mut self, page: Page) {
        if self.current() == &page {
            return;
        }
        self.stack.push(page);
        NAV_DEPTH.store(self.stack.len(), std::sync::atomic::Ordering::Relaxed);
    }

    /// 返回上一个页面；栈底不可弹出
    pub fn back(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            NAV_DEPTH.store(self.stack.len(), std::sync::atomic::Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn stack(&self) -> &[Page] {
        &self.stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `NAV_DEPTH` 是进程级原子量，测试并行执行会互相干扰，统一串行
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn push_and_back() {
        let _guard = SERIAL.lock();
        let mut nav = Navigator::new();
        assert_eq!(nav.current(), &Page::GalleryList);
        nav.push(Page::Details { gid: 1, token: "aaaaaaaaaa".into() });
        assert_eq!(nav.depth(), 2);
        nav.push(Page::Details { gid: 2, token: "bbbbbbbbbb".into() });
        assert_eq!(nav.depth(), 3);
        assert!(nav.back());
        assert_eq!(nav.current(), &Page::Details { gid: 1, token: "aaaaaaaaaa".into() });
        assert!(nav.back());
        assert!(!nav.back(), "栈底不能再退");
        assert_eq!(nav.depth(), 1);
        assert_eq!(NAV_DEPTH.load(std::sync::atomic::Ordering::Relaxed), 1, "原子量必须跟栈同步");
    }

    #[test]
    fn duplicate_push_is_ignored() {
        let _guard = SERIAL.lock();
        let mut nav = Navigator::new();
        let page = Page::Details { gid: 7, token: "cccccccccc".into() };
        nav.push(page.clone());
        nav.push(page);
        assert_eq!(nav.depth(), 2);
        assert_eq!(NAV_DEPTH.load(std::sync::atomic::Ordering::Relaxed), 2, "原子量必须跟栈同步");
    }
}
