// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

#![warn(rust_2018_idioms, missing_docs)]
#![warn(clippy::dbg_macro, clippy::print_stdout)]
#![doc = include_str!("../README.md")]

mod execute;
mod module;

use {
    crate::module::module_init,
    once_cell::sync::OnceCell,
    phper::{
        ini::{Ini, Policy},
        modules::Module,
        php_get_module,
    },
    std::{mem::forget, num::NonZeroUsize, sync::Arc, thread::available_parallelism},
    tokio::runtime::{self, Runtime},
};

const SKYWALKING_AGENT_ENABLE: &str = "skywalking_agent.enable";
const SKYWALKING_AGENT_WORKER_THREADS: &str = "skywalking_agent.worker_threads";

#[php_get_module]
pub fn get_module() -> Module {
    let mut module = Module::new(
        env!("CARGO_CRATE_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS"),
    );

    // Register skywalking_agent ini.
    Ini::add(SKYWALKING_AGENT_ENABLE, false, Policy::All);
    Ini::add(SKYWALKING_AGENT_WORKER_THREADS, 0i64, Policy::All);

    let rt = Arc::new(OnceCell::new());
    let rt_ = rt.clone();

    module.on_module_init(move |_| {
        let enable = Ini::get::<bool>(SKYWALKING_AGENT_ENABLE).unwrap_or_default();
        if enable {
            module_init();

            let guard = rt.get_or_init(new_tokio_runtime).enter();
            forget(guard);
        }
        true
    });
    module.on_module_shutdown(move |_| {
        drop(rt_);
        true
    });

    module
}

fn new_tokio_runtime() -> Runtime {
    let worker_threads = Ini::get::<i64>(SKYWALKING_AGENT_WORKER_THREADS).unwrap_or(0);
    let worker_threads = if worker_threads <= 0 {
        available_parallelism().map(NonZeroUsize::get).unwrap_or(1)
    } else {
        worker_threads as usize
    };

    runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(worker_threads)
        .build()
        .unwrap()
}
