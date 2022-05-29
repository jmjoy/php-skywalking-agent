// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::{module::is_ready_for_request, plugin::select_plugin};
use phper::{
    strings::ZendString,
    sys,
    values::{ExecuteData, Val},
};

static mut ORI_EXECUTE_INTERNAL: Option<
    unsafe extern "C" fn(execute_data: *mut sys::zend_execute_data, return_value: *mut sys::zval),
> = None;

unsafe extern "C" fn execute_internal(
    execute_data: *mut sys::zend_execute_data, return_value: *mut sys::zval,
) {
    if !is_ready_for_request() {
        ori_execute_internal(execute_data, return_value);
        return;
    }

    let execute = ExecuteData::from_mut_ptr(execute_data);

    let function = execute.func();
    let function_name = function.get_name();
    let function_name = match function_name.as_str() {
        Ok(function_name) => function_name,
        Err(_) => {
            ori_execute_internal(execute_data, return_value);
            return;
        }
    };

    let is_class = !(*function.as_ptr()).common.scope.is_null()
        && !((*(*function.as_ptr()).common.scope).name.is_null());
    let class_name = if is_class {
        ZendString::from_ptr((*(*function.as_ptr()).common.scope).name)
            .and_then(|s| s.as_str().ok())
    } else {
        None
    };

    let plugin = select_plugin(class_name, function_name);

    if let Some(plugin) = plugin {
        plugin.before_execute(execute, class_name, function_name);
    }

    ori_execute_internal(execute_data, return_value);

    if let Some(plugin) = plugin {
        plugin.after_execute(
            execute,
            Val::from_mut_ptr(return_value),
            class_name,
            function_name,
        );
    }
}

#[inline]
unsafe fn ori_execute_internal(
    execute_data: *mut sys::zend_execute_data, return_value: *mut sys::zval,
) {
    match ORI_EXECUTE_INTERNAL {
        Some(f) => f(execute_data, return_value),
        None => sys::execute_internal(execute_data, return_value),
    }
}

pub fn register_execute_functions() {
    unsafe {
        ORI_EXECUTE_INTERNAL = sys::zend_execute_internal;
        sys::zend_execute_internal = Some(execute_internal);
    }
}
