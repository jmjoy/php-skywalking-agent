// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::panic::{catch_unwind, UnwindSafe};

use chrono::Local;
use once_cell::sync::Lazy;
use phper::values::ZVal;
use systemstat::{IpAddr, Platform, System};
use tracing::error;

pub static IPS: Lazy<Vec<String>> = Lazy::new(|| {
    System::new()
        .networks()
        .ok()
        .and_then(|networks| {
            let addrs = networks
                .values()
                .map(|network| {
                    network
                        .addrs
                        .iter()
                        .filter_map(|network_addr| match network_addr.addr {
                            IpAddr::V4(addr) => {
                                if network.name == "lo"
                                    || network.name.starts_with("docker")
                                    || network.name.starts_with("br-")
                                {
                                    None
                                } else {
                                    Some(addr.to_string())
                                }
                            }
                            _ => None,
                        })
                })
                .flatten()
                .collect::<Vec<_>>();

            if addrs.is_empty() {
                None
            } else {
                Some(addrs)
            }
        })
        .unwrap_or_else(|| vec!["127.0.0.1".to_owned()])
});

pub static HOST_NAME: Lazy<String> = Lazy::new(|| {
    hostname::get()
        .ok()
        .and_then(|hostname| hostname.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string())
});

pub const OS_NAME: &str = if cfg!(target_os = "linux") {
    "Linux"
} else if cfg!(target_os = "windows") {
    "Windows"
} else if cfg!(target_os = "macos") {
    "Macos"
} else {
    "Unknown"
};

pub fn current_formatted_time() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn z_val_to_string(zv: &ZVal) -> Option<String> {
    zv.as_z_str()
        .and_then(|zs| zs.to_str().ok())
        .map(|s| s.to_string())
}

pub fn catch_unwind_and_log<F: FnOnce() + UnwindSafe>(f: F) {
    if let Err(e) = catch_unwind(f) {
        if let Some(s) = e.downcast_ref::<&str>() {
            error!(error = s, "paniced");
        } else if let Some(s) = e.downcast_ref::<String>() {
            error!(error = s, "paniced");
        } else {
            error!("paniced");
        }
    }
}
