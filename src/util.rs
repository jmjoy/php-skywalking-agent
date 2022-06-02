// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use once_cell::sync::Lazy;
use systemstat::{IpAddr, Platform, System};

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
