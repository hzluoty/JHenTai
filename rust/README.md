# JHenTai Rust（纯 Android arm64-v8a）

按 [`docs/rust-rewrite-plan.md`](../docs/rust-rewrite-plan.md) 重写的版本：业务与 UI 全在 Rust，Kotlin 只留一层薄壳。

## 目录

```
rust/
  crates/
    core/       纯 Rust：模型、解析、网络、数据库、设置、服务、图片
    platform/   Platform trait + Android(JNI)/host 两份实现
    app/        Slint UI、controller、Navigator；Android 入口与桌面入口
  scripts/      构建脚本
crates/app/android/  Gradle 工程（NativeActivity + Bridge.kt）
```

依赖方向：`app → platform/core`，`platform → core`。core 不含任何 Android/Slint 依赖，测试全部能在 host 上跑。

## 常用命令

```bash
# host 测试（不需要 Android 工具链）
cargo test --workspace

# 真实站点冒烟测试（需要网络/代理）
LIVE=1 HTTPS_PROXY=http://127.0.0.1:10808 cargo test -p jhentai-core --test live_network -- --ignored

# 桌面预览（Windows/Linux，platform 用 host stub）
cargo run -p jhentai

# 生成 arm64 动态库
cargo ndk -t arm64-v8a -P 24 -o crates/app/android/app/src/main/jniLibs build --release -p jhentai

# 打 APK
cd crates/app/android && gradle assembleRelease
# 产物：crates/app/android/app/build/outputs/apk/release/app-release.apk
```

`cargo ndk` 需要这几个环境变量（Slint 的 Java helper 要用 javac + d8）：

```
ANDROID_HOME=C:\Users\<you>\AppData\Local\Android\Sdk
ANDROID_NDK_HOME=%ANDROID_HOME%\ndk\27.3.13750724
ANDROID_JAR=%ANDROID_HOME%\platforms\android-36\android.jar
ANDROID_D8_JAR=%ANDROID_HOME%\build-tools\35.0.0\lib\d8.jar
```

## 模拟器 / 真机验证

```bash
adb install -r crates/app/android/app/build/outputs/apk/release/app-release.apk
# 代理：reqwest 不读系统代理，Bridge.systemProxy() 会读这一项
adb shell settings put global http_proxy 10.0.2.2:10808   # 模拟器指向宿主
adb shell am start -n top.jtmonster.jhentai/.MainActivity
```

## 当前进度

| 里程碑 | 状态 |
|---|---|
| M0 工具链 + spike | ✅ 工具链跑通；Slint 1.18.1 在模拟器上跑通列表 + 手势基础 |
| M1 core: model/parser/net | ✅ 解析器金样测试（真实页面 fixture）+ 域前置/代理/cookie |
| M2 core: db/config/services | 🟡 db v25 接管、`local_config`、账号/历史/阅读进度/搜索已完成；标签翻译、屏蔽规则未做 |
| M3 UI 骨架 + 浏览 | 🟡 导航、6 个 tab、列表卡片、封面、详情页（含缩略图/标签/评论预览、多页缩略图补全）已完成；搜索 UI、登录页、3 种卡片样式未做 |
| M4 阅读器 | 🟡 横向单页 + 点击翻页 + 预加载 ±1 + 图片缓存窗口 + 续读进度/多页缩略图已通 | 纵向/双页、缩放、音量键、自动翻页、亮度/常亮、保存分享未做 |
| M5 下载 | 🟡 画廊下载（并发/续传/metadata/ComicInfo）、下载页、离线阅读已通 | 归档下载、archive bot、本地画廊附加目录、前台服务通知 |
| M6 设置/安全/同步/i18n | 🟡 设置页（站点/代理/域前置/阅读/下载/安全 PIN/关于+AboutSlint）、6 语言、账号登录 | 标签集/我的标签、样式主题、屏蔽规则、标签翻译、云同步、生物识别、防截屏 |
| M7 切换与发布 | 🟡 发布清单见 `RELEASE.md` | 覆盖安装/回滚演练、真机性能、删除 Flutter 代码 |

跨线程约定（改代码前必读）：

- 网络/解码在 tokio 线程，结果只能通过 `ui_events`（`pages/pump.rs`）里的原子标志回到 UI 线程；`slint::Image`、`VecModel` 都是 `!Send`，只能在 UI 线程构造。
- 页栈 → UI 只走 `AppController::show_page`；返回键由 `MainActivity.dispatchKeyEvent` → `nativeOnBack` → `NAV_DEPTH`/`BACK_REQUESTED` 原子量 → UI 泵出栈。
- 异步回填写回模型前必须校验身份（列表按 gid、缩略图按 href、阅读器按 gid + 索引边界），
  否则切页会画错行；索引类容器还要校验 `index < len`，否则会 panic。
