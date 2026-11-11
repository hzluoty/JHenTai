# 与 Flutter 版的差异记录

基线：commit `9f8fcb1f`，v8.0.18+345，Drift schemaVersion 25，`jhentai.version = 13`。
本文件记录 Rust 版**有意**偏离 Dart 版的地方，以及上游改动需要手动追的位置。

## 有意偏离（行为不同，但等价或更严格）

| 位置 | Flutter | Rust | 原因 |
|---|---|---|---|
| 域前置 | 把 URL 域名换成 IP + 加 Host 头 + 放行所有证书 | 自定义 DNS 解析到轮询 IP，SNI/证书仍按原域名校验 | 更安全，且不改变 URL 语义（计划 §4.6） |
| 列表翻页 | 每个 tab 各自实现 | 统一 `PageCursor{First,Next(gid),Prev(gid),Index(p)}` | 修掉「Next 重载第一页」的隐患；排行页仍用页码 |
| 详情页字段 | 按固定行号取 `#gdd` 单元格 | 先按标签名（`Length`/`File Size`/`Language`/`Visible`）找，找不到再退回行号 | 现网页面行序与 Dart 代码假设不一致 |
| 列表时间字段 | `.gl2c > div:nth-child(2) > [id]` | 先试该选择器，再退 `.gl2c [id^=posted_]` | 同上，现网结构已变 |
| 标签解析 | 有 namespace 才拆 | 无 namespace 时归入 `temp`（与 Dart 一致），但 key 清洗用 `td_` 前缀裁剪 | 保持 i18n key 一致 |
| 图片解码 | extended_image 自动处理 | core::image 按显示宽度降采样解码 | 大图内存对策（计划 §7） |
| 阅读图片源 | 默认 `#img` 直链，可切换原图 | 只用 `#img` 直链 | `fullimg` 原图接口匿名访问会 302 到登录页；原图留给 M5 的「下载原图」 |
| 图片请求 | dio 默认 | 手动跟 302 + 带 `Referer` | 图床会 302 到 CDN 且校验来源 |

## 已删除（不移植，见计划 §2.1）

超分辨率、桌面/平板布局、桌面输入（键盘/鼠标/滚轮）、GetStorage、13 级迁移链、Sentry、
template_expressions/telegram、frame_rate_service。

## 上游需要手动追的文件

| Dart 文件 | 关注点 |
|---|---|
| `lib/src/utils/eh_spider_parser.dart` | 页面结构变化，改 Rust 的 `core/src/parser/eh*.rs` |
| `lib/src/network/eh_request.dart` | 请求头/cookie/错误处理变化 |
| `lib/src/service/gallery_download/*` | 下载状态机与 metadata 格式 |
| `lib/src/model/search_config.dart` | 搜索参数（`f_cats` 位掩码等） |
| `lib/src/setting/*.dart` | 配置字段名（`local_config` 的 JSON 键必须一致） |

## 数据兼容

- 只接受 `PRAGMA user_version = 25` 且 `jhentai.version = 13`；更低版本要求先装最后一个 Flutter 版跑完迁移。
- 首次打开先复制 `db.sqlite.flutter.bak`（已存在则不覆盖）。
- `local_config` 四列（configKey/subConfigKey/value/utime）与 Dart 版一致，设置 struct 用 serde rename 对齐字段名。
