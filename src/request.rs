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
    component::COMPONENT_PHP_ID,
    context::RequestContext,
    module::is_ready_for_request,
    util::{catch_unwind_and_log, z_val_to_string},
};
use anyhow::Context;
use phper::{
    arrays::ZArr,
    eg,
    modules::ModuleContext,
    pg, sg,
    sys::{self},
};
use skywalking::context::{
    propagation::decoder::decode_propagation,
    tracer::{self},
};
use tracing::{error, instrument, trace, warn};

#[instrument(skip_all)]
pub fn init(_module: ModuleContext) -> bool {
    if is_ready_for_request() {
        catch_unwind_and_log(|| request_init(None));
    }
    true
}

#[instrument(skip_all)]
pub fn shutdown(_module: ModuleContext) -> bool {
    if is_ready_for_request() {
        catch_unwind_and_log(|| request_flush(None));
    }
    true
}

fn request_init(request_id: Option<u64>) {
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

    trace!("Propagation: {:?}", &propagation);

    let mut ctx = match propagation {
        Some(propagation) => tracer::create_trace_context_from_propagation(propagation),
        None => tracer::create_trace_context(),
    };

    let operation_name = format!("{method}:{uri}");
    let mut span = ctx.create_entry_span(&operation_name);
    span.with_span_object_mut(|span| span.component_id = COMPONENT_PHP_ID);
    span.add_tag("url", &uri);
    span.add_tag("http.method", &method);

    RequestContext::set_global(
        request_id,
        RequestContext {
            tracing_context: ctx,
            entry_span: span,
        },
    );
}

fn request_flush(request_id: Option<u64>) {
    let RequestContext {
        tracing_context,
        mut entry_span,
    } = match RequestContext::remove_global(request_id) {
        Some(request_context) => request_context,
        None => return,
    };

    let status_code = unsafe { sg!(sapi_headers).http_response_code };
    entry_span.add_tag("http.status_code", &status_code.to_string());
    if status_code >= 400 {
        entry_span.with_span_object_mut(|span| span.is_error = true);
    }

    drop(entry_span);
    drop(tracing_context);
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

fn get_page_request_header(server: &ZArr) -> Option<String> {
    // TODO Support multi skywlaking version.
    server
        .get("HTTP_SW8")
        .and_then(|sw| sw.as_z_str())
        .and_then(|zs| zs.to_str().ok())
        .map(|s| s.to_string())
}

fn get_page_request_uri(server: &ZArr) -> String {
    server
        .get("REQUEST_URI")
        .and_then(z_val_to_string)
        .or_else(|| server.get("PHP_SELF").and_then(z_val_to_string))
        .or_else(|| server.get("SCRIPT_NAME").and_then(z_val_to_string))
        .unwrap_or_else(|| "{unknown}".to_string())
}

fn get_page_request_method(server: &ZArr) -> String {
    server
        .get("REQUEST_METHOD")
        .and_then(z_val_to_string)
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn get_page_request_server<'a>() -> anyhow::Result<&'a ZArr> {
    unsafe {
        let symbol_table = ZArr::from_mut_ptr(&mut eg!(symbol_table));
        let carrier = symbol_table
            .get("_SERVER")
            .and_then(|carrier| carrier.as_z_arr())
            .context("$_SERVER is null")?;
        Ok(carrier)
    }
}
