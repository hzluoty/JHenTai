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
        token: String,
        index: i64,
    },
    Downloads,
    Settings,
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
    }

    /// 返回上一个页面；栈底不可弹出
    pub fn back(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
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

    #[test]
    fn push_and_back() {
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
    }

    #[test]
    fn duplicate_push_is_ignored() {
        let mut nav = Navigator::new();
        let page = Page::Details { gid: 7, token: "cccccccccc".into() };
        nav.push(page.clone());
        nav.push(page);
        assert_eq!(nav.depth(), 2);
    }
}
