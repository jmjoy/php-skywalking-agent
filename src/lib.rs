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

mod channel;
mod component;
mod context;
mod execute;
mod module;
mod plugin;
mod request;
mod util;
mod worker;

use phper::{
    ini::{Ini, Policy},
    modules::Module,
    php_get_module,
};

/// Enable agent and report or not.
const SKYWALKING_AGENT_ENABLE: &str = "skywalking_agent.enable";

/// Version of skywalking server.
const SKYWALKING_AGENT_VERSION: &str = "skywalking_agent.version";

/// skywalking server address.
const SKYWALKING_AGENT_SERVER_ADDR: &str = "skywalking_agent.server_addr";

/// skywalking app service name.
const SKYWALKING_AGENT_SERVICE_NAME: &str = "skywalking_agent.service_name";

/// Tokio runtime worker threads.
const SKYWALKING_AGENT_WORKER_THREADS: &str = "skywalking_agent.worker_threads";

/// Log level of skywalking agent.
const SKYWALKING_AGENT_LOG_LEVEL: &str = "skywalking_agent.log_level";

/// Log file of skywalking agent.
const SKYWALKING_AGENT_LOG_FILE: &str = "skywalking_agent.log_file";

/// Max message length to report to skywalking.
const SKYWALKING_AGENT_MAX_MESSAGE_LENGTH: &str = "skywalking_agent.max_message_length";

#[php_get_module]
pub fn get_module() -> Module {
    let mut module = Module::new(
        env!("CARGO_CRATE_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS"),
    );

    // Register skywalking_agent ini.
    Ini::add(SKYWALKING_AGENT_ENABLE, false, Policy::System);
    Ini::add(SKYWALKING_AGENT_VERSION, 9i64, Policy::System);
    Ini::add(
        SKYWALKING_AGENT_SERVER_ADDR,
        "http://127.0.0.1:11800".to_string(),
        Policy::System,
    );
    Ini::add(
        SKYWALKING_AGENT_SERVICE_NAME,
        "hello-skywalking".to_string(),
        Policy::System,
    );
    Ini::add(SKYWALKING_AGENT_WORKER_THREADS, 0i64, Policy::System);
    Ini::add(
        SKYWALKING_AGENT_LOG_LEVEL,
        "OFF".to_string(),
        Policy::System,
    );
    Ini::add(
        SKYWALKING_AGENT_LOG_FILE,
        "/tmp/skywalking_agent.log".to_string(),
        Policy::System,
    );
    Ini::add(
        SKYWALKING_AGENT_MAX_MESSAGE_LENGTH,
        81920i64,
        Policy::System,
    );

    // Hooks.
    module.on_module_init(module::init);
    module.on_module_shutdown(module::shutdown);
    module.on_request_init(request::init);
    module.on_request_shutdown(request::shutdown);

    module
}
