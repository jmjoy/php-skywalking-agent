use crate::module::is_ready_for_request;
use phper::modules::ModuleContext;

pub fn init(_module: ModuleContext) -> bool {
    if is_ready_for_request() {}
    true
}

pub fn shutdown(_module: ModuleContext) -> bool {
    true
}
