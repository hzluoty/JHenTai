//! Android 平台实现：通过 JNI 调用 Kotlin 壳里的 `Bridge`。
//!
//! 所有方法都可能从 Rust 线程调用；Kotlin 侧负责切主线程（见 Bridge.kt）。
//! 未初始化 JavaVM（host 测试）时所有调用都退化为默认值，不 panic。

use std::path::PathBuf;
use std::sync::OnceLock;

use jni::objects::{JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

use crate::Platform;

/// 缓存的 JavaVM 与 Bridge 类引用。
///
/// 原生线程里 `find_class` 用的是系统 classloader，找不到 APK 里的类，
/// 因此走 `Context.getClassLoader().loadClass(...)`（ndk_context 提供 Activity 引用）。
static VM: OnceLock<JavaVM> = OnceLock::new();
static BRIDGE_CLASS: OnceLock<jni::objects::GlobalRef> = OnceLock::new();

const BRIDGE: &str = "top/jtmonster/jhentai/Bridge";

/// 兜底初始化：只缓存 VM；Bridge 类在首次使用时懒加载。
pub fn init(vm: JavaVM) {
    let _ = VM.set(vm);
}

/// 解析 Bridge 类：优先用应用自己的 ClassLoader，退化到 find_class。
fn bridge_class(env: &mut JNIEnv) -> Option<JClass<'static>> {
    if let Some(global) = BRIDGE_CLASS.get() {
        return Some(unsafe { JClass::from_raw(global.as_obj().as_raw()) });
    }

    let loaded = (|| -> jni::errors::Result<JObject> {
        let ctx = ndk_context::android_context();
        let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
        let loader = env
            .call_method(&activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
            .l()?;
        let name = env.new_string(BRIDGE.replace('/', "."))?;
        env.call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&JObject::from(name))],
        )?
        .l()
    })();

    let class: JObject = match loaded {
        Ok(obj) => obj,
        Err(_) => {
            // Activity 还没就绪时退回 find_class（主线程调用可用）
            let _ = env.exception_clear();
            match env.find_class(BRIDGE) {
                Ok(cls) => unsafe { JObject::from_raw(cls.into_raw()) },
                Err(_) => return None,
            }
        }
    };

    let global = env.new_global_ref(&class).ok()?;
    let raw = global.as_obj().as_raw();
    let _ = BRIDGE_CLASS.set(global);
    Some(unsafe { JClass::from_raw(raw) })
}

/// 在附着到当前线程的 JNIEnv 上执行（类引用懒加载并缓存）。
fn with_env<R>(f: impl FnOnce(&mut JNIEnv, JClass) -> Option<R>) -> Option<R> {
    let vm = VM.get()?;
    let mut env = vm.attach_current_thread().ok()?;
    let class = bridge_class(&mut env)?;
    let result = f(&mut env, class);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
}

fn call_str(name: &str, sig: &str, args: &[JValue]) -> Option<String> {
    let owned: Vec<JValue> = args.to_vec();
    with_env(|env, class| {
        let obj = env.call_static_method(class, name, sig, &owned).ok()?.l().ok()?;
        java_string_to_rust(env, obj)
    })
}

/// 把 Java String 读成 Rust String（空指针转成 None）
fn java_string_to_rust(env: &mut JNIEnv, obj: JObject) -> Option<String> {
    if obj.is_null() {
        return None;
    }
    let jstr = JString::from(obj);
    // 先落到局部变量，避免尾表达式的临时值延长借用
    let text = {
        let js = env.get_string(&jstr).ok()?;
        String::from(js)
    };
    Some(text)
}

fn call_bool(name: &str, sig: &str, args: &[JValue]) -> bool {
    let owned: Vec<JValue> = args.to_vec();
    with_env(|env, class| env.call_static_method(class, name, sig, &owned).ok()?.z().ok()).unwrap_or(false)
}

fn call_bool_with_strings(name: &str, sig: &str, s1: &str, s2: &str) -> bool {
    with_env(|env, class| {
        let a = env.new_string(s1).ok()?;
        let b = env.new_string(s2).ok()?;
        env.call_static_method(
            class,
            name,
            sig,
            &[JValue::Object(&JObject::from(a)), JValue::Object(&JObject::from(b))],
        )
        .ok()?
        .z()
        .ok()
    })
    .unwrap_or(false)
}

fn call_int(name: &str, sig: &str, args: &[JValue]) -> i32 {
    let owned: Vec<JValue> = args.to_vec();
    with_env(|env, class| env.call_static_method(class, name, sig, &owned).ok()?.i().ok()).unwrap_or(-1)
}

fn call_void(name: &str, sig: &str, args: &[JValue]) {
    let owned: Vec<JValue> = args.to_vec();
    with_env(|env, class| {
        let _ = env.call_static_method(class, name, sig, &owned);
        Some(())
    });
}

fn with_java_string(s: &str, f: impl FnOnce(&mut JNIEnv, JObject)) -> Option<()> {
    with_env(|env, _| {
        let jstr = env.new_string(s).ok()?;
        f(env, JObject::from(jstr));
        Some(())
    })
}

/// 两个字符串参数：借用 env 构造，闭包里用完即弃
fn with_two_strings(s1: &str, s2: &str, f: impl FnOnce(&mut JNIEnv, JClass, JValue, JValue)) {
    let _ = with_env(|env, class| {
        let a = env.new_string(s1).ok()?;
        let b = env.new_string(s2).ok()?;
        f(env, class, JValue::Object(&JObject::from(a)), JValue::Object(&JObject::from(b)));
        Some(())
    });
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AndroidPlatform;

impl Platform for AndroidPlatform {
    fn external_data_path(&self) -> PathBuf {
        PathBuf::from(call_str("externalFilesDir", "()Ljava/lang/String;", &[]).unwrap_or_default())
    }

    fn set_keep_screen_on(&self, on: bool) {
        // shortcut: 屏幕常亮由 app 层用 android-activity 的 set_window_flags 做，
        // 这里只留 Java 兜底，M5 接前台服务时一并实现。
        call_void("keepScreenOn", "(Z)V", &[JValue::Bool(on as u8)]);
    }

    fn set_secure_screen(&self, on: bool) {
        call_void("setSecureFlag", "(Z)V", &[JValue::Bool(on as u8)]);
    }

    fn set_screen_brightness(&self, brightness: Option<f32>) {
        call_void("setBrightness", "(F)V", &[JValue::Float(brightness.unwrap_or(-1.0))]);
    }

    fn battery_level(&self) -> Option<u8> {
        let level = call_int("batteryLevel", "()I", &[]);
        (0..=100).contains(&level).then_some(level as u8)
    }

    fn set_clipboard(&self, text: &str) {
        with_java_string(text, |env, jtext| {
            if let Ok(class) = env.find_class(BRIDGE) {
                let _ = env.call_static_method(class, "setClipboard", "(Ljava/lang/String;)V", &[JValue::Object(&jtext)]);
            }
        });
    }

    fn get_clipboard(&self) -> Option<String> {
        call_str("getClipboard", "()Ljava/lang/String;", &[])
    }

    fn open_url(&self, url: &str) {
        with_java_string(url, |env, jurl| {
            if let Ok(class) = env.find_class(BRIDGE) {
                let _ = env.call_static_method(class, "openUrl", "(Ljava/lang/String;)V", &[JValue::Object(&jurl)]);
            }
        });
    }

    /// 系统代理（`adb shell settings put global http_proxy host:port` 写入的那一项）
    fn system_proxy(&self) -> Option<String> {
        call_str("systemProxy", "()Ljava/lang/String;", &[]).filter(|p| !p.trim().is_empty())
    }

    fn save_image_to_gallery(&self, path: &PathBuf, mime: &str) -> bool {
        call_bool_with_strings("saveImage", "(Ljava/lang/String;Ljava/lang/String;)Z", &path.to_string_lossy(), mime)
    }

    fn share_file(&self, path: &PathBuf, mime: &str) {
        with_two_strings(&path.to_string_lossy(), mime, |env, class, a, b| {
            let _ = env.call_static_method(class, "shareFile", "(Ljava/lang/String;Ljava/lang/String;)V", &[a, b]);
        });
    }

    fn app_version(&self) -> String {
        call_str("appVersion", "()Ljava/lang/String;", &[]).unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
    }

    fn device_info(&self) -> String {
        call_str("deviceInfo", "()Ljava/lang/String;", &[]).unwrap_or_default()
    }

    fn has_all_files_access(&self) -> bool {
        call_bool("hasAllFilesAccess", "()Z", &[])
    }

    fn request_all_files_access(&self) {
        call_void("requestAllFilesAccess", "()V", &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_vm_every_call_degrades_to_default() {
        // 未初始化 JavaVM 时不得 panic
        let platform = AndroidPlatform;
        assert!(!platform.has_all_files_access());
        assert!(platform.battery_level().is_none());
        assert!(platform.system_proxy().is_none());
        assert!(platform.get_clipboard().is_none());
        assert!(!platform.save_image_to_gallery(&PathBuf::from("/tmp/x.jpg"), "image/jpeg"));
    }
}
