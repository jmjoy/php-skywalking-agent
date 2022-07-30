// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::{module::is_ready_for_request, plugin::select_plugin, util::catch_unwind_anyhow};
use anyhow::{bail, Context};
use phper::{
    objects::ZObj,
    strings::ZStr,
    sys,
    values::{ExecuteData, ZVal},
};
use std::{any::Any, panic::AssertUnwindSafe};
use tracing::error;

pub type BeforeExecuteHook = dyn FnOnce(&mut ExecuteData) -> anyhow::Result<Box<dyn Any>>;

pub type AfterExecuteHook = dyn FnOnce(Box<dyn Any>, &mut ExecuteData, &ZVal) -> anyhow::Result<()>;

pub trait Noop {
    fn noop() -> Self;
}

impl Noop for Box<BeforeExecuteHook> {
    #[inline]
    fn noop() -> Self {
        fn f(_: &mut ExecuteData) -> anyhow::Result<Box<dyn Any>> {
            Ok(Box::new(()))
        }
        Box::new(f)
    }
}

impl Noop for Box<AfterExecuteHook> {
    #[inline]
    fn noop() -> Self {
        fn f(_: Box<dyn Any>, _: &mut ExecuteData, _: &ZVal) -> anyhow::Result<()> {
            Ok(())
        }
        Box::new(f)
    }
}

static mut ORI_EXECUTE_INTERNAL: Option<
    unsafe extern "C" fn(execute_data: *mut sys::zend_execute_data, return_value: *mut sys::zval),
> = None;

#[tracing::instrument(skip_all)]
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

    let function_name = function.get_function_name();
    let function_name = match function_name.to_str().ok() {
        Some(s) => s,
        None => {
            ori_execute_internal(execute_data, return_value);
            return;
        }
    };
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
    let plugin = match plugin {
        Some(plugin) => plugin,
        None => {
            ori_execute_internal(execute_data, return_value);
            return;
        }
    };

    let (before, after) = match plugin.hook(class_name.as_deref(), &function_name) {
        Some(hook) => hook,
        None => {
            ori_execute_internal(execute_data, return_value);
            return;
        }
    };

    let result = catch_unwind_anyhow(AssertUnwindSafe(|| before(execute_data)));
    if let Err(e) = &result {
        error!("before execute: {:?}", e);
    }

    ori_execute_internal(execute_data, return_value);

    // If before hook return error, don't execute the after hook.
    if let Ok(data) = result {
        if let Err(e) =
            catch_unwind_anyhow(AssertUnwindSafe(|| after(data, execute_data, return_value)))
        {
            error!("after execute: {:?}", e);
        }
    }
}

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

pub fn validate_num_args(execute_data: &mut ExecuteData, num: usize) -> anyhow::Result<()> {
    if execute_data.num_args() < num {
        bail!("argument count incorrect");
    }
    Ok(())
}

pub fn get_this_mut(execute_data: &mut ExecuteData) -> anyhow::Result<&mut ZObj> {
    execute_data.get_this_mut().context("$this is empty")
}
