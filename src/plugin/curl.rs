// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::any;

use super::Plugin;
use crate::{
    component::COMPONENT_PHP_CURL_ID,
    execute::ExecuteInternal,
    request::{current_tracing_context, TRACING_CONTEXT_MAP},
};
use anyhow::{anyhow, Context};
use phper::{
    functions::call,
    sys,
    values::{ExecuteData, Val},
};
use tracing::{debug, error, warn};
use url::Url;

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

    fn execute(
        &self, execute_internal: ExecuteInternal, execute_data: &mut ExecuteData,
        return_value: &mut Val, _class_name: Option<&str>, function_name: &str,
    ) {
        match function_name {
            "curl_exec" => self.execute_curl_exec(execute_internal, execute_data, return_value),
            _ => execute_internal(execute_data, return_value),
        }
    }
}

impl CurlPlugin {
    #[tracing::instrument(skip_all)]
    fn execute_curl_exec(
        &self, execute_internal: ExecuteInternal, execute_data: &mut ExecuteData,
        return_value: &mut Val,
    ) {
        if unsafe { execute_data.num_args() } < 1 {
            execute_internal(execute_data, return_value);
            return;
        }

        let mut f = || {
            let ch = execute_data.get_parameter(1);
            let result = call("curl_getinfo", &mut [ch.clone()]).context("Call curl_get_info")?;
            let result = result.as_array()?;

            let url = result
                .get("url")
                .context("Get url from curl_get_info result")?;
            let raw_url = url.as_str()?;
            let mut url = raw_url.to_string();

            debug!("curl_getinfo get url: {}", &url);

            if !url.contains("://") {
                url.insert_str(0, "http://");
            }

            let url: Url = url.parse()?;
            if url.scheme() != "http" && url.scheme() != "https" {
                return Ok(None);
            }
            let host = match url.host_str() {
                Some(host) => host,
                None => return Ok(None),
            };
            let port = match url.port() {
                Some(port) => port,
                None => match url.scheme() {
                    "http" => 80,
                    "https" => 443,
                    _ => 0,
                },
            };

            let mut span = current_tracing_context()?
                .create_exit_span(url.path(), &format!("{host}:{port}"))
                .map_err(|e| anyhow!("Create exit span failed: {}", e))?;
            span.span_object_mut().component_id = COMPONENT_PHP_CURL_ID;
            span.add_tag(("url", raw_url));

            Ok::<_, anyhow::Error>(Some(span))
        };

        let result = f();
        if let Err(e) = &result {
            error!("{}", e);
        }

        execute_internal(execute_data, return_value);

        if let Ok(Some(mut span)) = result {
            let f = || {
                let ch = execute_data.get_parameter(1);
                let result =
                    call("curl_getinfo", &mut [ch.clone()]).context("Call curl_get_info")?;
                let response = result.as_array()?;
                let http_code = response
                    .get("http_code")
                    .and_then(|code| code.as_long().ok())
                    .context("Call curl_getinfo, http_code is null")?;
                span.add_tag(("status_code", &*http_code.to_string()));
                if http_code == 0 {
                    let result =
                        call("curl_error", &mut [ch.clone()]).context("Call curl_get_info")?;
                    let curl_error = result.as_str()?;
                    span.add_log(vec![("CURL_ERROR", curl_error)]);
                    span.span_object_mut().is_error = true;
                } else if http_code >= 400 {
                    span.span_object_mut().is_error = true;
                } else {
                    span.span_object_mut().is_error = false;
                }
                current_tracing_context()?.finalize_span(span);
                Ok::<_, anyhow::Error>(())
            };
            if let Err(e) = f() {
                error!("{}", e);
            }
        }
    }
}
