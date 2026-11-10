//! JNI surface for `md.vibecodium.mobile.CoreBridge`.
//!
//! The Java class is deliberately a plain final class with static native
//! methods: that keeps the generated symbol names stable
//! (`Java_md_vibecodium_mobile_CoreBridge_nativeXxx`) and the second parameter
//! a `JClass` instead of an object instance.

use std::sync::OnceLock;

use jni::objects::{JClass, JObject, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;

pub static JAVA_VM: OnceLock<jni::JavaVM> = OnceLock::new();

#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: jni::JavaVM, _reserved: *mut std::ffi::c_void) -> jni::sys::jint {
    let _ = JAVA_VM.set(vm);
    jni::sys::JNI_VERSION_1_6
}

pub fn rasterize_emoji(emoji: &str, px: f32) -> Option<(usize, usize, Vec<u32>)> {
    let vm = JAVA_VM.get()?;
    let mut env = vm.attach_current_thread().ok()?;
    let j_str = env.new_string(emoji).ok()?;
    let class = env.find_class("md/vibecodium/mobile/CoreBridge").ok()?;
    let result = env.call_static_method(
        class,
        "rasterizeEmoji",
        "(Ljava/lang/String;F)[I",
        &[(&j_str).into(), jni::objects::JValue::Float(px)],
    ).ok()?;
    let obj = result.l().ok()?;
    if obj.is_null() {
        return None;
    }
    let array: jni::objects::JIntArray = obj.into();
    let len = env.get_array_length(&array).ok()? as usize;
    if len < 2 {
        return None;
    }
    let mut buf = vec![0i32; len];
    env.get_int_array_region(&array, 0, &mut buf).ok()?;
    let w = buf[0] as usize;
    let h = buf[1] as usize;
    if w == 0 || h == 0 || len < 2 + w * h {
        return None;
    }
    let pixels: Vec<u32> = buf[2..2 + w * h].iter().map(|&p| p as u32).collect();
    Some((w, h, pixels))
}

fn to_java(env: &mut JNIEnv<'_>, value: String) -> jstring {
    match env.new_string(value) {
        Ok(string) => {
            let object: JObject<'_> = string.into();
            object.into_raw()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

fn from_java(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Result<String, String> {
    env.get_string(value)
        .map(|string| string.into())
        .map_err(|error| error.to_string())
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeVersion(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java(&mut env, crate::api::version())
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeOpenVault(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &path) {
        Ok(path) => crate::api::open_vault(&path),
        Err(error) => crate::api::err(format!("Could not read the vault path: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeCreateVault(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &path) {
        Ok(path) => crate::api::create_vault(&path),
        Err(error) => crate::api::err(format!("Could not read the vault path: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeCloseVault(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java(&mut env, crate::api::close_vault())
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeVaultRoot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java(&mut env, crate::api::vault_root())
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeListNotes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java(&mut env, crate::api::list_notes())
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeReadNote(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &path) {
        Ok(path) => crate::api::read_note(&path),
        Err(error) => crate::api::err(format!("Could not read the note path: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeWriteNote(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
    text: JString<'_>,
) -> jstring {
    let response = match (from_java(&mut env, &path), from_java(&mut env, &text)) {
        (Ok(path), Ok(text)) => crate::api::write_note(&path, &text),
        (Err(error), _) | (_, Err(error)) => {
            crate::api::err(format!("Could not read the note payload: {error}"))
        }
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeCreateNote(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
    text: JString<'_>,
) -> jstring {
    let response = match (from_java(&mut env, &path), from_java(&mut env, &text)) {
        (Ok(path), Ok(text)) => crate::api::create_note(&path, &text),
        (Err(error), _) | (_, Err(error)) => {
            crate::api::err(format!("Could not read the note payload: {error}"))
        }
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeDeleteNote(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &path) {
        Ok(path) => crate::api::delete_note(&path),
        Err(error) => crate::api::err(format!("Could not read the note path: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeRenameNote(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    old_path: JString<'_>,
    new_path: JString<'_>,
) -> jstring {
    let response = match (
        from_java(&mut env, &old_path),
        from_java(&mut env, &new_path),
    ) {
        (Ok(old_path), Ok(new_path)) => crate::api::rename_note(&old_path, &new_path),
        (Err(error), _) | (_, Err(error)) => {
            crate::api::err(format!("Could not read the rename paths: {error}"))
        }
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearch(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    query: JString<'_>,
    limit: jint,
) -> jstring {
    let response = match from_java(&mut env, &query) {
        Ok(query) => crate::api::search(&query, limit.max(1) as u32),
        Err(error) => crate::api::err(format!("Could not read the search query: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeOutline(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    text: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &text) {
        Ok(text) => crate::api::outline(&text),
        Err(error) => crate::api::err(format!("Could not read the note text: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeStats(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    text: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &text) {
        Ok(text) => crate::api::stats(&text),
        Err(error) => crate::api::err(format!("Could not read the note text: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeMarkdown(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    text: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &text) {
        Ok(text) => crate::api::markdown(&text),
        Err(error) => crate::api::err(format!("Could not read the note text: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeAnalyze(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    text: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &text) {
        Ok(text) => crate::api::analyze(&text),
        Err(error) => crate::api::err(format!("Could not read the note text: {error}")),
    };
    to_java(&mut env, response)
}

// ------------------------------------------------------------ indexed search

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchIndexReset(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java(&mut env, crate::api::search_index_reset())
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchIndexAdd(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
    text: JString<'_>,
    mtime: jlong,
    len: jlong,
) -> jstring {
    let response = match (from_java(&mut env, &path), from_java(&mut env, &text)) {
        (Ok(path), Ok(text)) => crate::api::search_index_add(
            &path,
            &text,
            mtime.max(0) as u64,
            len.max(0) as u64,
        ),
        (Err(error), _) | (_, Err(error)) => {
            crate::api::err(format!("Could not read the note payload: {error}"))
        }
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchIndexRemove(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &path) {
        Ok(path) => crate::api::search_index_remove(&path),
        Err(error) => crate::api::err(format!("Could not read the note path: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchIndexFinish(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java(&mut env, crate::api::search_index_finish())
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchIndexStatus(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    to_java(&mut env, crate::api::search_index_status())
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchIndexLoad(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &path) {
        Ok(path) => crate::api::search_index_load(&path),
        Err(error) => crate::api::err(format!("Could not read the snapshot path: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchIndexSave(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) -> jstring {
    let response = match from_java(&mut env, &path) {
        Ok(path) => crate::api::search_index_save(&path),
        Err(error) => crate::api::err(format!("Could not read the snapshot path: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchIndexQuery(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    query: JString<'_>,
    limit: jint,
) -> jstring {
    let response = match from_java(&mut env, &query) {
        Ok(query) => crate::api::search_index_query(&query, limit.max(1) as u32),
        Err(error) => crate::api::err(format!("Could not read the search query: {error}")),
    };
    to_java(&mut env, response)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSearchLocate(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    query: JString<'_>,
    text: JString<'_>,
) -> jstring {
    let response = match (from_java(&mut env, &query), from_java(&mut env, &text)) {
        (Ok(query), Ok(text)) => crate::api::search_locate(&query, &text),
        (Err(error), _) | (_, Err(error)) => {
            crate::api::err(format!("Could not read the search payload: {error}"))
        }
    };
    to_java(&mut env, response)
}
