use phper::{strings::ZendString, sys, values::ExecuteData};

static mut ORI_EXECUTE_INTERNAL: Option<
    unsafe extern "C" fn(execute_data: *mut sys::zend_execute_data, return_value: *mut sys::zval),
> = None;

unsafe extern "C" fn execute_internal(
    execute_data: *mut sys::zend_execute_data, return_value: *mut sys::zval,
) {
    let execute = ExecuteData::from_mut_ptr(execute_data);
    let func = execute.func();
    let is_class = !(*func.as_ptr()).common.scope.is_null()
        && !((*(*func.as_ptr()).common.scope).name.is_null());
    let class_name = if is_class {
        ZendString::from_ptr((*(*func.as_ptr()).common.scope).name).and_then(|s| s.as_str().ok())
    } else {
        None
    };
    let func_name = func.get_name();
    let func_name = func_name.as_str().ok();

    dbg!(class_name, func_name);

    ori_execute_internal(execute_data, return_value);
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

pub fn exchange_execute() {
    unsafe {
        ORI_EXECUTE_INTERNAL = sys::zend_execute_internal;
        sys::zend_execute_internal = Some(execute_internal);
    }
}
