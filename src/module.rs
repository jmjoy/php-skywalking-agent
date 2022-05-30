// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::{
    execute::register_execute_functions, report::init_reporter, SKYWALKING_AGENT_ENABLE,
    SKYWALKING_AGENT_LOG_FILE, SKYWALKING_AGENT_LOG_LEVEL,
};
use ipc_channel::ipc::IpcSharedMemory;
use once_cell::sync::OnceCell;
use phper::{ini::Ini, modules::ModuleContext, sys};
use std::{
    ffi::CStr,
    intrinsics::transmute,
    mem::size_of,
    ops::Deref,
    path::Path,
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};
use tracing::metadata::LevelFilter;
use tracing_subscriber::FmtSubscriber;

pub fn init(_module: ModuleContext) -> bool {
    // Now only support in FPM mode.
    if get_sapi_module_name().to_bytes() != b"fpm-fcgi" {
        return true;
    }

    let enable = Ini::get::<bool>(SKYWALKING_AGENT_ENABLE).unwrap_or_default();
    if enable {
        init_logger();
        get_ready_for_request();
        register_execute_functions();
        init_reporter();
    }
    true
}

pub fn shutdown(_module: ModuleContext) -> bool {
    true
}

pub fn is_ready_for_request() -> bool {
    get_ready_for_request().load(Ordering::SeqCst)
}

pub fn mark_ready_for_request() {
    get_ready_for_request().store(true, Ordering::SeqCst)
}

/// Share memory to store is ready for request tag.
fn get_ready_for_request() -> &'static AtomicBool {
    static READY_FOR_REQUEST: OnceCell<IpcSharedMemory> = OnceCell::new();
    let ready = READY_FOR_REQUEST.get_or_init(|| {
        let b: [u8; size_of::<AtomicBool>()] = unsafe { transmute(AtomicBool::new(false)) };
        IpcSharedMemory::from_bytes(&b)
    });
    let ready: &[u8] = ready.deref();
    let ready = ready.as_ptr() as *const AtomicBool;
    unsafe { ready.as_ref().unwrap() }
}

fn init_logger() {
    let log_level =
        Ini::get::<String>(SKYWALKING_AGENT_LOG_LEVEL).unwrap_or_else(|| "INFO".to_string());
    let log_level = log_level.trim();

    let log_file = Ini::get::<String>(SKYWALKING_AGENT_LOG_FILE).unwrap_or_else(|| "".to_string());
    let log_file = log_file.trim();

    if !log_file.is_empty() {
        if let Ok(log_level) = LevelFilter::from_str(&log_level) {
            let log_file = Path::new(log_file);
            if let Some(dir) = log_file.parent() {
                if let Some(file_name) = log_file.file_name() {
                    let file_appender = tracing_appender::rolling::never(dir, file_name);
                    let subscriber = FmtSubscriber::builder()
                        .with_max_level(log_level)
                        .with_writer(file_appender)
                        .finish();

                    tracing::subscriber::set_global_default(subscriber)
                        .expect("setting default subscriber failed");
                }
            }
        }
    }
}

fn get_sapi_module_name() -> &'static CStr {
    unsafe { CStr::from_ptr(sys::sapi_module.name) }
}
