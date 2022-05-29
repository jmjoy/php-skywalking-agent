// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use super::Plugin;
use phper::values::ExecuteData;

#[derive(Default)]
pub struct CurlPlugin {}

impl Plugin for CurlPlugin {
    #[inline]
    fn class_names(&self) -> Option<&'static [&'static str]> {
        None
    }

    #[inline]
    fn function_name_prefix(&self) -> Option<&'static str> {
        Some("curl_")
    }

    fn before_execute(
        &self, _execute_data: &mut ExecuteData, class_name: Option<&str>, function_name: &str,
    ) {
        dbg!(class_name, function_name);
    }
}
