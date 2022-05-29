// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

mod curl;

use once_cell::sync::OnceCell;
use phper::values::{ExecuteData, Val};

pub trait Plugin {
    fn class_names(&self) -> Option<&'static [&'static str]>;

    fn function_name_prefix(&self) -> Option<&'static str>;

    fn before_execute(
        &self, execute_data: &mut ExecuteData, class_name: Option<&str>, function_name: &str,
    );

    #[allow(unused_variables)]
    fn after_execute(
        &self, execute_data: &mut ExecuteData, return_value: &mut Val, class_name: Option<&str>,
        function_name: &str,
    ) {
    }
}

pub type DynPlugin = dyn Plugin + Send + Sync + 'static;

fn get_plugins() -> &'static [Box<DynPlugin>] {
    static PLUGINS: OnceCell<Vec<Box<DynPlugin>>> = OnceCell::new();
    PLUGINS.get_or_init(|| vec![Box::new(curl::CurlPlugin::default())])
}

pub fn select_plugin(class_name: Option<&str>, function_name: &str) -> Option<&'static DynPlugin> {
    let mut selected_plugin = None;

    for plugin in get_plugins() {
        if let Some(class_name) = class_name {
            if let Some(plugin_class_names) = plugin.class_names() {
                if plugin_class_names.contains(&class_name) {
                    selected_plugin = Some(plugin);
                    break;
                }
            }
        }
        if let Some(function_name_prefix) = plugin.function_name_prefix() {
            if function_name.starts_with(function_name_prefix) {
                selected_plugin = Some(plugin);
                break;
            }
        }
    }

    selected_plugin.map(AsRef::as_ref)
}
