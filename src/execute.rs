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
use helper::try_option;
use phper::{
    strings::ZStr,
    sys,
    values::{ExecuteData, ZVal},
};

static mut ORI_EXECUTE_INTERNAL: Option<
    unsafe extern "C" fn(execute_data: *mut sys::zend_execute_data, return_value: *mut sys::zval),
> = None;

unsafe extern "C" fn execute_internal(
    execute_data: *mut sys::zend_execute_data, return_value: *mut sys::zval,
) {
    if !is_ready_for_request() {
        raw_ori_execute_internal(execute_data, return_value);
        return;
    }

    let execute_data = ExecuteData::from_mut_ptr(execute_data);
    let return_value = ZVal::from_mut_ptr(return_value);

    let function = execute_data.func();

    let function_name = function.get_name();
    let function_name = try_option!(function_name.to_str().ok() ? {
        ori_execute_internal(execute_data, return_value);
    });
    let function_name = function_name.to_owned();

    let is_class = !(*function.as_ptr()).common.scope.is_null()
        && !((*(*function.as_ptr()).common.scope).name.is_null());
    let class_name = if is_class {
        ZStr::from_ptr((*(*function.as_ptr()).common.scope).name)
            .to_str()
            .ok()
    } else {
        None
    };
    let class_name = class_name.map(ToOwned::to_owned);

    let plugin = select_plugin(class_name.as_deref(), &function_name);

    if let Some(plugin) = plugin {
        plugin.execute(
            ori_execute_internal,
            execute_data,
            return_value,
            class_name.as_deref(),
            &function_name,
        );
    } else {
        ori_execute_internal(execute_data, return_value);
    }
}

pub type ExecuteInternal = fn(execute_data: &mut ExecuteData, return_value: &mut ZVal);

#[inline]
fn ori_execute_internal(execute_data: &mut ExecuteData, return_value: &mut ZVal) {
    unsafe { raw_ori_execute_internal(execute_data.as_mut_ptr(), return_value.as_mut_ptr()) }
}

#[inline]
unsafe fn raw_ori_execute_internal(
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
