# 发布说明 / Release checklist

版本：`rust/` crate 版本 `0.1.0`，applicationId `top.jtmonster.jhentai`（与 Flutter 版一致，可覆盖安装）。

## 当前可发布范围（RC）

已实现并**在模拟器上验证过**：

- 列表：首页 / 热门 / 排行 / 收藏 / 关注，游标翻页（`next=`/`prev=` gid），封面异步加载
- 详情：标题/日文标题/分类/页数/评分/上传者/语言/体积/时间、标签按命名空间分组、缩略图（多页自动抓全）、评论预览
- 阅读：横向单页、点击左右 1/3 翻页、±1 预加载、图片缓存滑动窗口、续读进度入库、屏幕常亮
- 下载：画廊下载（并发、断点续传、魔数校验、metadata + ComicInfo.xml 落盘）、下载页进度、**离线阅读**
- 设置：站点 EH/EX（未登录时自动回退 EH 并提示）、代理 + 域前置、阅读方向/常亮、下载并发/原图、应用锁 PIN、语言（6 种）、关于（含 AboutSlint）
- 账号：账号密码登录、cookie 导入、登出（保存到 `local_config.eh_cookies`，与 Dart 版同 key）
- 数据：直接接管 Flutter 版 `db.sqlite`（v25）、一次性 `.bak` 备份、旧版拒绝并提示

## 发布前必做（人工）

1. `cargo test --workspace` 全绿（当前 10 个测试套件 / 73 项 + 3 项 live/ignored）
2. `cargo ndk -t arm64-v8a ... && gradle assembleRelease`，产物 `crates/app/android/app/build/outputs/apk/release/app-release.apk`
3. 用**真实 Flutter 版数据**演练覆盖安装：装 Flutter 版 → 登录/下载/阅读 → 覆盖装 Rust 版 → 检查设置、阅读进度、下载目录 metadata 可读
4. 回滚演练：卸载 Rust 版 → 装回 Flutter 版 → 确认 `.bak` 可用
5. 真机（arm64）跑：冷启动时间、列表滑动 fps、长篇阅读 RSS（`dumpsys meminfo`）、下载 20 页以上画廊

## 已知缺口（对照计划 §2.3 功能对齐清单）

| 区域 | 缺口 |
|---|---|
| 阅读 | 纵向/双页布局、捏合与双击缩放、亮度调节、点击区域配置、第三方查看器（音量键翻页与自动翻页已通）（保存/分享已通：`save`/`share` 在阅读器顶栏，PNG 编码在阻塞线程池，保存到 `Pictures/JHenTai`） |
| 列表 | 瀑布流卡片、缩略图卡片样式、下拉刷新、跳页输入、标签联想（关键字搜索已通，含 gid 游标翻页与历史记录） |
| 详情 | 投票/加标签、收藏分组与备注（默认槽位收藏/取消已通，需登录）、评论发布/修改/投票、种子列表、归档解锁、统计页、关注标签 |
| 下载 | 归档（archiver）下载、archive bot、本地画廊扫描附加目录、分组与排序、前台服务通知、原图批量下载（需登录） |
| 设置 | 标签集 / 我的标签、样式（主题色/列表样式）、屏蔽规则引擎、标签翻译（EhTagTranslation）、云同步/分享码、日志与缓存、更新检查 |
| 安全 | 生物识别、防截屏（`set_window_flags` 已留接口未接） |
| 平台 | WebView 登录、分享接收/深链、音量键 JNI、前台服务、保存到相册（Bridge 已实现但未接 UI） |
| i18n | 主界面/tab/设置页/阅读器关键文案已接 6 语言；详情页若干标签、错误提示与占位文案仍是英文（`ui-*` 属性机制已就绪，逐条接线即可） |
| 工程 | 真机性能基线、CI 只在 Linux 上验证过构建脚本、M7 删除 Flutter 代码未执行（等对齐） |

## 工程债（发布前建议处理）

- `ui/app.slint` 是单文件（5 个页面内联在 `if page == N` 下）。第 20 轮曾因按行号截断丢失过整个详情页且**编译不报错**；
  现在加了 `crates/app/tests/ui_structure.rs` 结构守卫（每个页面块存在、关键接线仍引用、块不重复、括号配平），
  但计划 §4.3 的「每页一个 component」拆分仍未做——下一步 UI 功能之前应完成。
- `reader_error` 目前兼任「保存/分享结果提示」，M6 做通知/状态条时应拆开。
- 搜索时顶栏改为显示 `Search: <keyword>`（已修）。
- 自动翻页目前是运行时开关（不持久化）；M6 收尾时可并入 `ReadSetting` 一起保存。

## 与 Flutter 版的数据兼容

- 同 `applicationId` + 同签名 → 覆盖安装直接看到旧 `db.sqlite`（v25）、`download/`、`local_gallery/`
- 只接受 `PRAGMA user_version = 25` 且 `jhentai.version = 13`；更低版本要求先装最后一个 Flutter 版
- 首次打开先复制 `db.sqlite.flutter.bak`（已存在不覆盖）
- 下载目录布局：`<download>/<gid> - <sanitizedTitle>/<serialNo>.<ext>` + `metadata` + `ComicInfo.xml`，字段名与 Dart 版逐字一致（`images` 同样是二次编码的字符串）
