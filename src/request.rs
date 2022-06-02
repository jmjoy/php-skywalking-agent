// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::{
    channel::channel_send,
    module::{is_ready_for_request, SERVICE_INSTANCE, SERVICE_NAME},
};
use dashmap::{mapref::one::RefMut, DashMap};
use once_cell::sync::OnceCell;
use phper::modules::ModuleContext;
use skywalking_rust::context::trace_context::TracingContext;
use tracing::{debug, error, warn};

pub fn init(_module: ModuleContext) -> bool {
    if is_ready_for_request() {
        warn!(
            "&SERVICE_NAME: {}, &SERVICE_INSTANCE: {}",
            &*SERVICE_NAME, &*SERVICE_INSTANCE
        );
        get_tracing_context_map()
            .insert(0, TracingContext::default(&SERVICE_NAME, &SERVICE_INSTANCE));
        let mut ctx = get_tracing_context(0);
        match ctx.create_entry_span("hello") {
            Ok(span) => ctx.finalize_span(span),
            Err(e) => error!("create entry_span: {}", e),
        }
    }
    true
}

pub fn shutdown(_module: ModuleContext) -> bool {
    if is_ready_for_request() {
        for context in get_tracing_context_map() {
            debug!("{:?}", &*context);
        }
    }

    if let Err(e) = channel_send(b"Test channel") {
        error!("Channel send failed: {}", e);
    }

    true
}

fn get_tracing_context_map() -> &'static DashMap<u64, TracingContext> {
    static TRACING_CONTEXT_MAP: OnceCell<DashMap<u64, TracingContext>> = OnceCell::new();
    TRACING_CONTEXT_MAP.get_or_init(|| DashMap::new())
}

pub fn get_tracing_context(id: u64) -> RefMut<'static, u64, TracingContext> {
    get_tracing_context_map().get_mut(&id).unwrap()
}
