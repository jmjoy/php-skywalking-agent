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
    channel::{self, channel_send},
    component::COMPONENT_PHP_ID,
    module::{is_ready_for_request, SERVICE_INSTANCE, SERVICE_NAME},
};
use anyhow::{anyhow, Context};
use dashmap::{mapref::one::RefMut, DashMap};
use once_cell::sync::{Lazy, OnceCell};
use phper::{arrays::Array, eg, modules::ModuleContext, pg, sg, sys};
use prost::Message;
use skywalking_rust::context::{
    propagation::decoder::decode_propagation, trace_context::TracingContext,
};
use tonic::server;
use tracing::{debug, error, trace, warn};

// TODO Support cli mode(swoole), so use dashmap to store trace context.
pub static TRACING_CONTEXT_MAP: Lazy<DashMap<u64, TracingContext>> = Lazy::new(|| DashMap::new());

pub fn init(_module: ModuleContext) -> bool {
    if is_ready_for_request() {
        request_init(0);
    }
    true
}

pub fn shutdown(_module: ModuleContext) -> bool {
    if is_ready_for_request() {
        request_flush(0);
    }

    // if let Err(e) = channel_send(b"Test channel") {
    //     error!("Channel send failed: {}", e);
    // }

    true
}

#[tracing::instrument(skip_all)]
fn request_init(request_id: u64) {
    jit_initialization();

    let server = match get_page_request_server() {
        Ok(server) => server,
        Err(e) => {
            error!("Get $_SERVER failed: {}", e);
            return;
        }
    };

    let header = get_page_request_header(server);
    let uri = get_page_request_uri(server);
    // let peer = get_page_request_peer(server);
    let method = get_page_request_method(server);

    let propagation = header
        .as_ref()
        .and_then(|header| match decode_propagation(&header) {
            Ok(propagation) => Some(propagation),
            Err(e) => {
                error!("Decode propagation failed: {}", e);
                None
            }
        });

    let mut ctx = match propagation {
        Some(propagation) => {
            TracingContext::from_propagation_context(&SERVICE_NAME, &SERVICE_INSTANCE, propagation)
        }
        None => TracingContext::default(&SERVICE_NAME, &SERVICE_INSTANCE),
    };

    let operation_name = format!("{method}:{uri}");
    let mut span = match ctx.create_entry_span(&operation_name) {
        Ok(span) => span,
        Err(e) => {
            error!("Create entry span failed: {}", e);
            return;
        }
    };
    span.span_object_mut().component_id = COMPONENT_PHP_ID;
    span.add_tag(("url", &uri));
    span.add_tag(("http.method", &method));
    ctx.spans.push(span);

    TRACING_CONTEXT_MAP.insert(request_id, ctx);
}

fn request_flush(request_id: u64) {
    let mut tracing_context = match TRACING_CONTEXT_MAP.remove(&request_id) {
        Some((_, tracing_context)) => tracing_context,
        None => return,
    };
    let span = match tracing_context.spans.first_mut() {
        Some(span) => span,
        None => return,
    };
    let status_code = unsafe { sg!(sapi_headers).http_response_code };
    span.add_tag(("http.status_code", &status_code.to_string()));
    if status_code >= 400 {
        span.span_object_mut().is_error = true;
    }
    span.close();

    let segment = tracing_context.convert_segment_object();
    trace!("Trace segment: {:?}", segment);

    let message = segment.encode_to_vec();
    if message.len() > *channel::MAX_LENGTH {
        warn!(
            message_len = message.len(),
            max_message_length = *channel::MAX_LENGTH,
            "Message is too big"
        );
        return;
    }

    if let Err(e) = channel_send(&message) {
        error!("Channel send failed: {}", e);
    }
}

fn jit_initialization() {
    unsafe {
        let jit_initialization = pg!(auto_globals_jit);
        if jit_initialization != 0 {
            let mut server = "_SERVER".to_string();
            sys::zend_is_auto_global_str(server.as_mut_ptr().cast(), server.len() as sys::size_t);
        }
    }
}

fn get_page_request_header(server: &Array) -> Option<String> {
    // TODO Support multi skywlaking version.
    server.get("HTTP_SW8").and_then(|sw| sw.as_string().ok())
}

fn get_page_request_uri(server: &Array) -> String {
    server
        .get("REQUEST_URI")
        .and_then(|u| u.as_string().ok())
        .or_else(|| server.get("PHP_SELF").and_then(|u| u.as_string().ok()))
        .or_else(|| server.get("SCRIPT_NAME").and_then(|u| u.as_string().ok()))
        .unwrap_or_else(|| "{unknown}".to_string())
}

// fn get_page_request_peer(server: &Array) -> String {
//     let host = server
//         .get("HTTP_HOST")
//         .and_then(|s| s.as_string().ok())
//         .or_else(|| server.get("SERVER_ADDR").and_then(|s|
// s.as_string().ok()));     let port = server.get("SERVER_PORT").and_then(|s|
// s.as_string().ok());

//     match (host, port) {
//         (Some(host), Some(port)) => {
//             if host.contains(':') {
//                 host
//             } else {
//                 format!("{host}:{port}")
//             }
//         }
//         _ => "".to_string(),
//     }
// }

fn get_page_request_method(server: &Array) -> String {
    server
        .get("REQUEST_METHOD")
        .and_then(|u| u.as_string().ok())
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn get_page_request_server<'a>() -> anyhow::Result<&'a Array> {
    unsafe {
        let symbol_table =
            Array::from_mut_ptr(&mut eg!(symbol_table)).context("EG(symbol_table) is null")?;
        let carrier = symbol_table
            .get("_SERVER")
            .and_then(|carrier| carrier.as_array().ok())
            .context("$_SERVER is null")?;
        Ok(carrier)
    }
}

pub fn current_tracing_context() -> anyhow::Result<RefMut<'static, u64, TracingContext>> {
    TRACING_CONTEXT_MAP
        .get_mut(&0)
        .context("Current tracing context not exists")
}
