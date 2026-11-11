# JHenTai Rust 重写规划（纯 Android arm64-v8a）

> 基线：commit `9f8fcb1f`，v8.0.18+345，Drift `schemaVersion 25`，`jhentai.version = 13`。规划日期 2026-11-11。
> 状态：规划稿。UI 框架在 M0 真机 spike 通过后才锁定。

## 0. 结论

| 项 | 决定 |
|---|---|
| 目标 | 只出一个 Android arm64-v8a APK，业务和 UI 全部用 Rust；minSdk 24 / targetSdk 36 |
| UI | Slint ≥ 1.18.1（1.19 发布后升级，可播放 GIF/WebP 动图，Android 惯性滚动也按平台习惯）；M0 不通过就走方案 B |
| 非 Rust 部分 | Slint 自带的 Java helper（输入法）+ 自己写的 Kotlin 薄壳，≤ 600 行，不放业务逻辑 |
| 数据 | 同 applicationId、同签名覆盖安装；直接接管 `db.sqlite`(v25)、`local_config`、下载目录里的 `metadata`；**不写任何遗留迁移代码** |
| 工作量 | 约 27 人周（24–34），8 个里程碑，从 M3 起每个里程碑都能装 APK |
| 许可 | Slint Royalty-free 2.0：在“关于”页放 `AboutSlint` 就行，开源、闭源、免费应用都能用；不选 GPLv3 那一支 |

### 0.1 先质疑需求
1. **如果只是想要“纯安卓 arm64”包，不需要重写。** Flutter 现有工程改三处就够，半天工作量，功能不倒退：`flutter build apk --release --target-platform android-arm64`、`abiFilters 'arm64-v8a'`、删掉 ios/macos/windows/linux 目录和桌面分支。
2. 值得重写的理由只有两类：APK 体积、内存、冷启动；或者想摆脱 Dart/Flutter 以及 15 个以上插件 fork 的维护负担。代价是功能冻结大约半年，上游 jiangtian616/JHenTai 的改动要手动追。
3. **“完全 Rust”有硬边界。** Android 要求 Service、Activity 回调（onNewIntent、按键分发、权限结果）和 BiometricPrompt 回调必须是 Java 子类，所以 Kotlin 薄壳免不了。坚持 0 行 Kotlin，就得砍掉后台下载（切到后台会被杀）、生物识别、WebView 登录、分享接收/深链、音量键翻页。不建议这样做。

## 1. 现状盘点

Dart 手写代码（不含生成文件）共 66,306 行、420 个文件：

| 目录 | 行数 | Rust 去向 |
|---|---|---|
| pages | 27,438 | app（`.slint` + controller） |
| service | 9,302 | core::service / core::download |
| widget | 9,152 | app/ui/components |
| l18n | 5,715 | `.po`（用工具转换） |
| utils | 3,386 | core::parser / core::util |
| setting | 2,913 | core::config |
| model | 2,458 | core::model |
| network | 2,007 | core::net |
| database | 1,415 | core::db |
| config/routes/mixin/extension/enum/consts/exception | ~2,300 | 分散到各模块 |

其中路由 57 条，GetX Controller 36 个。

**大文件不按 1:1 搬**：`eh_spider_parser.dart` 1538 行、`gallery_download_service.dart` 2032 行、`archive_download_service.dart` 1461 行、`eh_request.dart` 1071 行、`read_page_logic.dart` 925 行。可以照着学的是 `service/gallery_download/`，已经拆成 service、task_runner、metadata_store、upgrade_migrator、path_resolver、images_retainer、exception_matcher 7 个文件，Rust 下载引擎按同样粒度拆；归档下载也照这个拆。

## 2. 范围

### 2.1 删除清单（不移植）

| 项 | 证据或理由 |
|---|---|
| 超分辨率 `super_resolution_service`（587 行）及设置页 | 只支持 Windows/macOS/Linux 可执行文件 |
| `layout/desktop`、`layout/tablet_v2` | 只做手机布局，平板推迟 |
| `windows_service`、window_manager、desktop_webview_window、flutter_resizable_container、system_network_proxy_{linux,windows,macos} | 桌面专属 |
| 键盘快捷键、鼠标滚轮设置页，`eh_keyboard_listener`、`eh_mouse_button_listener`、`eh_wheel_*` | 桌面输入 |
| `isolate_service`（integral_isolates） | Rust 直接用线程 |
| GetStorage（`storage_service`、`jhentai.gs`） | 只剩迁移源：`storageService.` 只在 `database.dart`、`app_update_service.dart` 里出现；最新 Flutter 版已经把数据迁进数据库 |
| `app_update_service` 的 13 级迁移链 | 只支持从最后一个 Flutter 版本升级（见 §4.4） |
| `config/sentry_config.dart` | `log.dart:101` 写着 “sentry is removed” |
| template_expressions、telegram | `lib/` 下没有任何 import |
| `frame_rate_service` + flutter_displaymode | Slint 1.18 的动画已跟随屏幕刷新率；真要手动高刷，再用 `ANativeWindow_setFrameRate`，推迟 |

### 2.2 推迟（功能对齐之后再做）
平板/折叠屏双栏、统计图表（先用表格）、高刷选项、动图（等 Slint 1.19）。

### 2.3 保留（功能对齐清单，即验收依据）
- **账号**：账号密码登录、导入 Cookie、WebView 登录、登出、EX（igneous）
- **列表**：首页/热门/排行/收藏/关注/历史；列表、瀑布流、缩略图三种卡片；下拉刷新、前后翻页、跳页；标签/上传者/评论屏蔽规则和内置屏蔽用户
- **搜索**：关键字和高级配置、标签联想（带翻译）、搜索历史、快速搜索、以图搜图（`requestLookup`）、在下载内容里搜索
- **详情**：信息、标签、投票加标签、评分、收藏（含备注和排序）、评论（发布、修改、投票）、缩略图（新版，以及旧版雪碧图）、种子、归档（解锁、取消、H@H）、统计、关注标签
- **阅读**：纵向列表、横向列表、横向单页、横向双页；左到右/右到左；捏合和双击缩放；点击区域；音量键；自动翻页；亮度、常亮、电量和时间栏；跳页；保存/分享图片；重载（nl）；509 与配额处理；数据源包括在线、已下载、本地画廊、归档
- **下载**：画廊下载（并发、暂停、恢复、优先级、分组、排序、原图、新版本更新）；归档下载（含 archive bot）；本地画廊扫描和额外扫描路径；从 metadata 恢复；ComicInfo.xml；网格和列表视图；前台服务通知
- **设置**：账户、EH 站点设置、标签集、我的标签、样式（主题色、暗色、列表样式）、偏好（屏蔽规则）、阅读（点击区域）、下载、网络（代理、域前置、超时）、性能、安全（PIN 应用锁、生物识别、防截屏）、云同步（配置导入导出、分享码）、高级（日志、缓存）、关于
- **服务**：标签翻译（EhTagTranslation）、标签搜索排序（csv）、历史、搜索历史、阅读进度、计划任务、更新检查、日志
- **语言**：en_US、zh_CN、zh_TW、ko_KR、pt_BR、ru_RU

## 3. 技术选型

### 3.1 UI 框架

| 方案 | Android 现状 | 对本项目的问题 | 结论 |
|---|---|---|---|
| **Slint** | 1.5 起有官方 android-activity 后端；Java helper 负责输入法组合输入（中日文）、选择柄、复制粘贴菜单；1.16 新增 `ScaleRotateGestureHandler`；master 源码里 Android 后端按 pointer id 分发多点触控，core 用双指触摸合成 Pinch 事件；另有 FlexboxLayout、StyledText、`Key.Back`；1.19 支持动图和 Android 惯性滚动 | 没有现成的缩放图片查看器、瀑布流、分组列表，都要自己写；Android 上渲染固定用 Skia，体积增加数 MB（M0 实测） | **首选，需 M0 真机验证** |
| Makepad | Robrix 已在 Android 上跑 | DSL 正在改成运行时脚本（Splash），API 变动大，文档少 | 不选 |
| Dioxus 0.7 | 用系统 WebView 渲染 | UI 本质是 HTML/CSS，阅读器手势还得写 JS，不算“纯 Rust” | 不选 |
| egui | 能跑 | 没有惯性滚动，触屏手感差 | 不选 |
| 方案 B：Compose + Rust core（UniFFI） | 最成熟（瀑布流、缩放都有现成组件，UI 工作量约少 20%） | UI 是 Kotlin，不符合“完全 Rust” | spike 失败时的退路；core crate 不用改 |

“master 源码确认”只能说明 Slint 有这个能力，1.18.1 发布版是否已经包含、真机手感如何，以 M0 实测为准，不凭 changelog 定框架。

**许可**：Slint 提供 GPLv3、Royalty-free 2.0、商业三种许可。选 Royalty-free 2.0：它允许把 Slint 作为移动应用的一部分免费分发，开源和闭源都可以，条件是在“关于”页显示 `AboutSlint` 控件（或在下载页放 Made with Slint 徽章）。它禁止用于嵌入式设备、禁止对外暴露 Slint API，本项目都不涉及。JHenTai 自己的 Apache-2.0 许可不受影响。

### 3.2 其他依赖

| 用途 | crate |
|---|---|
| 异步 | tokio（多线程） |
| HTTP | reqwest（rustls、socks、stream、cookies）+ cookie_store |
| HTML | scraper（API 和 Dart html 包的 querySelector 一一对应，方便逐函数移植） |
| 数据库 | rusqlite（bundled） |
| 序列化 | serde / serde_json（字段名和 Dart `toJson` 保持一致） |
| 图片 | image（jpeg/png/webp/gif）+ fast_image_resize |
| 压缩包 / XML / CSV / 时间 | zip / quick-xml（ComicInfo.xml）/ csv / chrono |
| 日志 | tracing + tracing-appender（滚动文件）+ 输出到 logcat |
| 哈希 / UUID | sha1、sha2、md-5 / uuid |
| Android | android-activity（native-activity）+ jni + ndk + ndk-context |
| 构建 | cargo-ndk + Gradle（Kotlin 壳） |

## 4. 架构

### 4.1 线程模型
- `android_main` 线程跑 Slint 事件循环，它就是 Slint 的 UI 线程（不是 Java 主线程）。所有 Slint 模型（Rc / VecModel，都是 `!Send`）只能在这个线程修改。
- tokio 多线程运行时负责网络、IO、图片解码。UI 侧用 `slint::spawn_local` 等 tokio 的 JoinHandle，后台任务用 `slint::invoke_from_event_loop` 把结果推回 UI。
- 必须在 Java 主线程执行的调用（亮度、剪贴板、WebView、BiometricPrompt）由 Kotlin 壳内部 `runOnUiThread`。
- 所有 JNI 入口都包 `catch_unwind`；release 保留 `panic = "unwind"`。

### 4.2 仓库布局（迁移期和 Flutter 代码并存，M7 删除 Flutter）

```
rust/
  Cargo.toml                 # workspace
  crates/
    core/      # 纯 Rust，能在 Windows/Linux 上跑全部测试
      model/
      parser/{list,detail,comment,image_page,favorite,torrent,archive,setting,mytags,stats,misc}.rs
      net/{client,cookie,fronting,cache,eh,jh,archive_bot}.rs
      db/{schema,migrate,dao/*}.rs
      config/*.rs
      service/*.rs
      download/gallery/{service,task_runner,metadata_store,path_resolver,images_retainer,exception_matcher}.rs
      download/archive/*.rs   download/{local,comic_info}.rs
      image/{cache,decode,sprite}.rs
    platform/  # trait Platform：android 用 JNI 实现；host stub 供桌面跑 UI 和测试
    app/       # cdylib（libjhentai.so）+ 桌面 bin；ui/*.slint、controller/*.rs、nav.rs
  android/     # Gradle：MainActivity（继承 NativeActivity）、DownloadService、LoginActivity、Bridge.kt
  tools/
    dart-golden/   # 在旧工程里跑 Dart 解析器，输出 golden JSON
    l10n/          # lib/src/l18n/*.dart → .po
```

只分 3 个 crate，划分标准是能不能脱离 Android 和 Slint 单独测试。如果 core 编译太慢再拆，到时在代码里留 `shortcut:` 注释。

### 4.3 UI 层
- 单 Window，加一个 Rust 写的 `Navigator` 页栈，替代 GetX 的 57 条路由；`Key.Back` 出栈。每个页面 = `.slint` 组件 + Controller，替代原来的 `*_logic` / `*_state`。
- `BasePageLogic` 改成泛型 `GalleryListController<S: GalleryListSource>`，6 个列表页只需实现各自的数据源。
- **自研组件**（各算独立工作项，M0 先做出雏形）：
  1. **ZoomableImage**：捏合（ScaleRotateGestureHandler）、双击缩放、拖拽、惯性、边界回弹，并处理和翻页手势的冲突
  2. **ReaderPager**：4 种布局、右到左、预加载窗口、跳转
  3. **MasonryGrid**：布局在 Rust 里算，只把可见项交给模型，实现虚拟化瀑布流
  4. **PositionedList**：按下标跳转（纵向阅读、缩略图、下载列表都要用）
  5. PullToRefresh、Slidable（SwipeGestureHandler）、DragReorder（下载分组排序）
  6. PinInput（应用锁）、ColorPicker（主题色）、Toast、BottomSheet、TagWrap（FlexboxLayout）
- **i18n**：用 Slint 的 `@tr()` 加打包进程序的 `.po`，运行时用 `select_bundled_translation` 切换语言；Rust 侧要显示的文案也通过 Slint global 函数取，翻译只有一个来源。

### 4.4 数据连续性
- 持久化仍然只有两处：SQLite 的 `db.sqlite`（业务表，加上 `local_config` 一张表存全部设置、cookie、快速搜索、阅读进度、搜索配置），以及下载目录里的 `metadata` JSON。**不再引入第三个 KV 存储。**
- 路径不变：`getExternalFilesDir(null)` 下的 `db.sqlite`、`download/`、`local_gallery/`、`save/`、`jhentai.version`（Rust 里用 `AndroidApp::external_data_path()` 取）。
- 覆盖安装：applicationId 用 `top.jtmonster.jhentai`，签名不变，新版就能直接看到旧文件。
- 首次启动：若 `jhentai.version == 13` 且 `PRAGMA user_version == 25`，先复制一份 `db.sqlite.flutter.bak` 再直接打开；从 v26 起由 Rust 负责迁移。版本更低的，提示先装最后一个 Flutter 版本跑完迁移，**所以不写遗留迁移代码，也不读 `jhentai.gs`**。
- schema 以 `drift_dev schema dump` 的输出和真机数据库的 `sqlite_master` 为准。`gallery_downloaded` 等表历史上新旧列名混用，不能从 Dart 表定义推出来。
- `local_config` 的四列 `configKey / subConfigKey / value(JSON) / utime` 保持不变；各个设置 struct 用 serde rename 对齐 Dart 字段名。
- 开发包加 `applicationIdSuffix ".rs"`，和正式数据隔离。测试数据靠旧版的“配置导出”，再加上扫描下载目录的 metadata 来恢复。

### 4.5 平台绑定层（platform crate + Kotlin 壳，作为一等模块）

| Flutter 依赖 | 用途 | 替代方案 | 实现位置 | 风险 |
|---|---|---|---|---|
| webview_flutter | WebView 登录取 cookie、打开站内网页 | LoginActivity（WebView + CookieManager），通过 JNI 回传 cookie；其他网页交给外部浏览器 | Kotlin | **高** |
| receive_sharing_intent，manifest 的 VIEW/SEND | 接收分享链接、深链打开 | MainActivity 的 onCreate/onNewIntent 经 JNI 交给 Navigator | Kotlin | **高** |
| volume_button_override，MainActivity 的 MethodChannel | 音量键翻页 | 在 MainActivity.dispatchKeyEvent 里按开关拦截，经 JNI 通知 Rust | Kotlin | **高** |
| local_auth | 生物识别解锁 | BiometricPrompt（AuthenticationCallback 必须是 Java 子类） | Kotlin | 中 |
| permission_handler | 存储、通知权限 | MANAGE_EXTERNAL_STORAGE 跳系统设置页，onResume 时复查；POST_NOTIFICATIONS 用 requestPermissions | JNI + Kotlin | 中 |
| file_picker | 选目录、选文件 | 已有全部文件访问权限，用应用内目录浏览器（std::fs）；没授权时只能用应用私有目录 | Rust/Slint | 中 |
| saver_gallery | 保存到相册 | API 29+ 用 MediaStore insert；API 24–28 写 Pictures 目录后调 MediaScanner | JNI | 低 |
| share_plus | 分享链接、图片、日志 | ACTION_SEND + androidx FileProvider（在 manifest 声明即可） | JNI | 低 |
| screen_brightness | 阅读时调亮度 | 设置 `window.attributes.screenBrightness` | Kotlin | 低 |
| wakelock_plus / flutter_windowmanager_plus | 屏幕常亮 / 防截屏 | android-activity 的 `set_window_flags(KEEP_SCREEN_ON / SECURE)` | Rust | 低 |
| battery_plus | 显示电量 | 用 `registerReceiver(null, ACTION_BATTERY_CHANGED)` 读粘性广播 | JNI | 低 |
| android_intent_plus / url_launcher | 打开系统设置页、外部链接 | Intent / Slint 的 open-url | JNI | 低 |
| path_provider / package_info_plus / device_info_plus | 目录、版本号、设备信息 | `AndroidApp::external_data_path` 等 / `env!` / `Build.*` | Rust/JNI | 低 |
| clipboard / pasteboard | 剪贴板 | TextInput 自带，加上 ClipboardManager | JNI | 低 |
| system_network_proxy | 读系统代理 | ConnectivityManager.getDefaultProxy | JNI | 低 |
| fluttertoast | 提示消息 | Slint 写的 Toast | Slint | 低 |
| （新增）后台下载 | 切到后台不被杀 | DownloadService 前台服务（dataSync 类型），Rust 推送进度来更新通知；处理 Android 15 对 dataSync 的 6 小时上限（onTimeout） | Kotlin | 中 |

其他非平台依赖的替代：

| Flutter 依赖 | 替代 |
|---|---|
| dio fork、j_downloader、retry、executor | reqwest + Range 断点续传 + Semaphore 控并发 + 退避重试 |
| extended_image、photo_view fork、zoom_view fork、zoom_widget | core::image + ZoomableImage |
| scrollable_positioned_list fork、flutter_list_view、waterfall_flow、extended_nested_scroll_view、grouped_list | Slint ListView + MasonryGrid / PositionedList |
| flutter_slidable、flutter_draggable_gridview | Slidable / DragReorder |
| flutter_widget_from_html（评论 HTML） | 把 HTML 解析成 span，用 StyledText 显示，图片单独成行 |
| syncfusion_flutter_charts | 先用表格 |
| drift / logger | rusqlite / tracing |
| html、xml、csv、crypto、uuid、archive、intl | scraper、quick-xml、csv、sha*、uuid、zip、chrono |
| pinput、flex_color_picker、like_button、animate_do、simple_animations、animated_flip_counter、loading_animation_widget、blur、dotted_border、flutter_rating_bar | 用 Slint 自己画 |

### 4.6 网络要点
- 同时支持 EH/EX 两站。cookie（ipb_member_id、ipb_pass_hash、igneous、sk 等）存在 `local_config`，key 和 Dart 版相同。
- **域前置**：Dart 版是把 URL 里的域名换成 IP、加 Host 头、用 `badCertificateCallback` 放行证书（`eh_request.dart:91/134`）。Rust 版改成：自定义 DNS 解析把域名指向轮询 IP（照搬 RoundRobinIpProvider，不可用的 IP 冷却 5 分钟），rustls 设 `enable_sni = false`，证书仍按原域名校验，比原版安全；证书对不上时，只对 e-hentai/exhentai 白名单放行。
- 缓存：沿用 `dio_cache` 表，语义不变（只缓存详情页和列表的 GET 请求，TTL 一致）。
- 代理：支持 HTTP、HTTPS、SOCKS5（用 socks5h，由代理端解析域名）。

## 5. 里程碑

| # | 内容 | 交付物与验收标准 | 估时（人周） |
|---|---|---|---|
| M0 | 工具链 + spike（决定 go/no-go） | 见 §5.1 | 1.5 |
| M1 | core：model + parser + net | 解析器金样测试全绿：至少 30 份脱敏 fixture，与 Dart 输出逐字段一致；用 mock server 测 cookie、登录、509、重试、域前置、代理；可选 live 测试（用环境变量提供 cookie） | 3 |
| M2 | core：db + config + services | 用真机导出的 v25 数据库做 fixture，测试读写；`local_config` 里所有设置能往返；标签翻译能下载入库；屏蔽规则引擎移植完成；现有 Dart 测试改写成 Rust 测试 | 2.5 |
| M3 | UI 骨架 + 浏览（Alpha1） | Navigator、底部导航、6 个列表页、3 种卡片、搜索和联想、详情页及各对话框、登录和 Cookie 页；真机目标：滑动 ≥ 55fps，冷启动 < 1.0s | 5 |
| M4 | 阅读器（Alpha2） | 4 种布局、手势、预加载、点击区域、音量键、自动翻页、亮度/常亮/状态栏、保存和分享、阅读进度与历史 | 4 |
| M5 | 下载（Beta1） | 画廊、归档、本地三类下载；前台服务通知；暂停恢复、优先级、分组、排序；metadata 和旧版能互相读；ComicInfo.xml；离线阅读 | 4 |
| M6 | 设置、站点设置、安全、同步、i18n（Beta2，即功能对齐） | §2.3 清单逐条勾完；6 种语言；主题色；应用锁、生物识别、防截屏 | 5 |
| M7 | 切换与发布 | 用真机上的旧版数据演练覆盖安装，包括回滚（装回 Flutter 版仍能读 `.bak`）；性能和内存回归；删除 Flutter 代码，把 Gradle 工程移到 `android/`，重写 README 和 CLAUDE.md；发布 | 2 |
| | **合计** | | **约 27（24–34）** |

### 5.1 M0 详细内容
1. **安装工具链**：Android SDK cmdline-tools、platform 36、build-tools、NDK r27c LTS 或更新；`rustup target add aarch64-linux-android`；`cargo install cargo-ndk`；设置 `ANDROID_HOME` / `ANDROID_NDK_HOME`。Slint 构建时要用 javac 和 d8 编译它的 Java helper；skia-bindings 构建时要从 GitHub 下载预编译包，需设 `HTTPS_PROXY=http://127.0.0.1:10808`。
2. **冻结基线**：打 tag `flutter-final`。顺手更新 CLAUDE.md 里过时的内容：版本应为 8.0.18+345，schema 是 25 不是 24，画廊下载服务已移到 `service/gallery_download/` 目录。
3. **骨架**：`rust/` workspace；Gradle 工程（NativeActivity 子类 + Bridge.kt）；Gradle 任务调用 `cargo ndk -t arm64-v8a -P 24 -o app/src/main/jniLibs build --release`；GitHub Actions 跑 host 测试并打 arm64 APK。
4. **Spike**：在一台中端机和一台低端机上实测，全部通过才锁定 Slint：
   - 输入法：搜狗和 Gboard 的中文拼音组合输入、候选、删除、选择柄、粘贴，在搜索框里都正常
   - 列表：1000 项瀑布流加网络 WebP 缩略图，滚动 ≥ 55fps，内存 < 300MB
   - 阅读器：捏合、双击缩放、拖拽和翻页互不冲突；2400px 原图不卡
   - Back 键、深链/分享 intent、音量键拦截、前台服务各跑通一次
   - 冷启动 < 1.0s；APK ≤ 15MB
   - 不通过就走方案 B（Compose UI + 同一套 core，通过 UniFFI 调用），M1/M2 的工作不受影响

## 6. 测试与回归
- **Dart 金样**：`tools/dart-golden` 用 `flutter test` 跑 EHSpiderParser 输出 JSON，Rust 测试逐字段比对。fixture 里去掉用户名、cookie、收藏夹名。
- **每个评审点配一个回归测试。** 现有 4 个 Dart 测试（download_path_resolver、download_setting、horizontal_safe_area、tap_zone_config）中能对应上的，先改写成 Rust 测试。
- **数据库**：用真机导出的 v25 数据库做 fixture，比较迁移前后的行数和关键字段。
- **网络 / 下载**：本地起 HTTP 服务器模拟 EH，覆盖 509、断点续传、慢速、断网等情况。
- **UI**：用 Slint testing backend（`init_integration_test_with_mock_time` + `ElementHandle::find_by_accessible_label`）做 controller 冒烟测试。app 同时编译桌面版，在 Windows 上快速迭代，platform 用 host stub。
- **真机**：每个里程碑一份 adb 验收清单，记录 fps、内存、冷启动时间。

## 7. 风险

| 风险 | 对策 |
|---|---|
| Slint 在 Android 上的手势、输入法、滚动不达标 | M0 设门槛；不通过就走方案 B，只换 UI |
| E-H 页面改版 | 金样测试兜底；每周跟踪上游 `eh_spider_parser.dart` 的改动并移植 |
| 大图占内存 | 按显示尺寸降采样解码；内存 LRU 按字节计；阅读器只保留前后 N 页的纹理；收到 LowMemory 事件时释放 GPU 资源 |
| 升级丢数据 | 首次启动先备份；只接受 v25 / 13；M7 在真机上演练升级和回滚 |
| 后台下载被杀 / Android 15 的 dataSync 超时 | 用前台服务；onTimeout 时保存进度，回到前台后续传 |
| 上游功能还在更新 | 基线冻结在 `9f8fcb1f`；差异记在 `rust/UPSTREAM.md`，功能对齐后再追 |
| Windows 构建链（NDK、skia 预编译包下载、javac/d8） | M0 跑通一次并写成脚本；CI 用 Linux |
| 许可 | Slint 用 Royalty-free，在关于页放 AboutSlint；第三方 crate 用 cargo-deny 检查 |

## 8. 下一步（M0）
1. 安装 Android 工具链，写好 `rust/scripts/setup-android.ps1`
2. 打 tag `flutter-final`，修正 CLAUDE.md 的过时内容
3. 搭 `rust/` workspace、Gradle 壳和 CI
4. 完成 4 项 spike，写 spike 报告（fps、内存、启动时间、APK 体积，附录屏），据此在 Slint 和方案 B 之间二选一

## 9. 实施进度（滚动更新）

代码在 `rust/`，说明见 `rust/README.md`，与 Flutter 版的差异见 `rust/UPSTREAM.md`。

| 里程碑 | 状态 | 已完成 | 待办 |
|---|---|---|---|
| M0 | ✅ | 工具链（NDK r27c / cargo-ndk / JDK21 / Gradle 8.14）、host 测试、arm64+x86_64 .so、APK、模拟器冒烟（列表/详情/封面/返回键/代理） | 真机 fps 与冷启动测量、`setup-android.ps1` |
| M1 | ✅ | model 全套、`eh_spider_parser` 全量移植、金样测试（真实页面 fixture）、reqwest 客户端、cookie 管理、域前置（自定义 DNS）、代理、509/封禁/Cloudflare 错误映射、live 冒烟测试 | 归档 bot、搜索建议的完整字段 |
| M2 | 🟡 | db v25 接管（含备份/版本双拒绝）、`local_config` 读写、NetworkSetting、账号登录/导入/登出、阅读进度、历史表、搜索历史/快速搜索 | 其余 12 个 setting struct、标签翻译服务、屏蔽规则引擎、`local_config` 全量往返测试 |
| M3 | 🟡 | Navigator + 返回键、6 tab 列表、卡片、封面管线、游标翻页、详情页（标签/缩略图/评论预览）、主题配色 | 搜索 UI 与标签联想、登录页与 WebView、3 种卡片样式、瀑布流、下拉刷新、评论交互 |
| M4 | 🟡 | 横向单页阅读、点击分区翻页、±1 预加载、图片缓存滑动窗口、续读进度入库、多页缩略图抓全 | 纵向/双页布局、ZoomableImage 缩放、音量键、自动翻页、亮度/常亮、保存分享、原图（需登录） |
| M5 | ⬜ | — | 画廊/归档/本地三类下载、前台服务、metadata 互通、离线阅读 |
| M6 | ⬜ | — | 设置页全量、安全（PIN/生物识别/防截屏）、云同步、6 语言 i18n |
| M7 | ⬜ | — | 覆盖安装演练、性能回归、删除 Flutter 代码、README/CLAUDE.md 重写、发布 |

当前测试：`cargo test --workspace` 9 个测试套件全绿（core 41 项 + 金样 6 项 + app 9 项），
`live_network` 2 项默认 ignore，需要真实网络/代理时手动启用。
