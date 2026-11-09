//! JNI surface for `md.vibecodium.mobile.CoreBridge`.
//!
//! The Java class is deliberately a plain final class with static native
//! methods: that keeps the generated symbol names stable
//! (`Java_md_vibecodium_mobile_CoreBridge_nativeXxx`) and the second parameter
//! a `JClass` instead of an object instance.

use jni::objects::{JClass, JObject, JString};
use jni::sys::{jint, jstring};
use jni::JNIEnv;

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
