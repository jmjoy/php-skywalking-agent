// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use super::Plugin;
use crate::{
    component::COMPONENT_PHP_PDO_ID,
    context::RequestContext,
    execute::{get_this_mut, validate_num_args, AfterExecuteHook, BeforeExecuteHook, Noop},
};
use anyhow::Context;
use phper::{
    arrays::ZArr,
    objects::ZObj,
    sys,
    values::{ExecuteData, ZVal},
};
use skywalking::{context::span::Span, skywalking_proto::v3::SpanLayer};
use std::{any::Any, cell::RefCell, collections::HashMap, str::FromStr};
use tracing::{debug, warn};

thread_local! {
    static DSN_MAP: RefCell<HashMap<u32, Dsn>> = Default::default();
    static DTOR_MAP: RefCell<HashMap<u32, sys::zend_object_dtor_obj_t>> = Default::default();
}

#[derive(Default, Clone)]
pub struct PdoPlugin;

impl Plugin for PdoPlugin {
    fn class_names(&self) -> Option<&'static [&'static str]> {
        static NAMES: &[&str] = &["PDO", "PDOStatement"];
        Some(NAMES)
    }

    fn function_name_prefix(&self) -> Option<&'static str> {
        None
    }

    fn hook(
        &self, class_name: Option<&str>, function_name: &str,
    ) -> Option<(
        Box<crate::execute::BeforeExecuteHook>,
        Box<crate::execute::AfterExecuteHook>,
    )> {
        match (class_name, function_name) {
            (Some("PDO"), "__construct") => Some(self.hook_pdo_construct()),
            (Some("PDO"), f)
                if [
                    "exec",
                    "query",
                    "prepare",
                    "commit",
                    "begintransaction",
                    "rollback",
                ]
                .contains(&f) =>
            {
                Some(self.hook_pdo_methods(function_name))
            }
            (Some("PDOStatement"), f)
                if ["execute", "fetch", "fetchAll", "fetchColumn", "fetchObject"].contains(&f) =>
            {
                Some(self.hook_pdo_statement_methods(function_name))
            }
            _ => None,
        }
    }
}

impl PdoPlugin {
    fn hook_pdo_construct(&self) -> (Box<BeforeExecuteHook>, Box<AfterExecuteHook>) {
        (
            Box::new(|execute_data| {
                validate_num_args(execute_data, 1)?;

                let this = get_this_mut(execute_data)?;
                let handle = this.handle();
                hack_dtor(this, Some(pdo_dtor));

                let dsn = execute_data.get_parameter(0);
                let dsn = dsn.as_z_str().context("dsn isn't str")?.to_str()?;
                debug!(dsn, "construct PDO");

                let dsn: Dsn = dsn.parse()?;
                debug!(?dsn, "parse PDO dsn");

                DSN_MAP.with(|dsn_map| {
                    dsn_map.borrow_mut().insert(handle, dsn);
                });

                Ok(Box::new(()))
            }),
            Noop::noop(),
        )
    }

    fn hook_pdo_methods(
        &self, function_name: &str,
    ) -> (Box<BeforeExecuteHook>, Box<AfterExecuteHook>) {
        let function_name = function_name.to_owned();
        (
            Box::new(move |execute_data| {
                let handle = get_this_mut(execute_data)?.handle();

                debug!(handle, function_name, "call PDO method");

                let mut span = with_dsn(handle, |dsn| {
                    create_exit_span_with_dsn("PDO", &function_name, dsn)
                })?;

                if execute_data.num_args() >= 1 {
                    if let Some(statement) = execute_data.get_parameter(0).as_z_str() {
                        span.add_tag("db.statement", statement.to_str()?);
                    }
                }

                Ok(Box::new(span) as _)
            }),
            Box::new(after_hook),
        )
    }

    fn hook_pdo_statement_methods(
        &self, function_name: &str,
    ) -> (Box<BeforeExecuteHook>, Box<AfterExecuteHook>) {
        let function_name = function_name.to_owned();
        (
            Box::new(move |execute_data| {
                let this = get_this_mut(execute_data)?;
                let handle = this.handle();

                debug!(handle, function_name, "call PDOStatement method");

                let mut span = with_dsn(handle, |dsn| {
                    create_exit_span_with_dsn("PDOStatement", &function_name, dsn)
                })?;

                if let Some(query) = this.get_property("queryString").as_z_str() {
                    span.add_tag("db.statement", query.to_str()?);
                } else {
                    warn!("PDOStatement queryString is empty");
                }

                Ok(Box::new(span) as _)
            }),
            Box::new(after_hook),
        )
    }
}

fn hack_dtor(this: &mut ZObj, new_dtor: sys::zend_object_dtor_obj_t) {
    let handle = this.handle();

    unsafe {
        let ori_dtor = (*(*this.as_mut_ptr()).handlers).dtor_obj;
        DTOR_MAP.with(|dtor_map| {
            dtor_map.borrow_mut().insert(handle, ori_dtor);
        });
        (*((*this.as_mut_ptr()).handlers as *mut sys::zend_object_handlers)).dtor_obj = new_dtor;
    }
}

unsafe extern "C" fn pdo_dtor(object: *mut sys::zend_object) {
    debug!("call PDO dtor");
    dtor(object);
}

unsafe extern "C" fn pdo_statement_dtor(object: *mut sys::zend_object) {
    debug!("call PDOStatement dtor");
    dtor(object);
}

unsafe extern "C" fn dtor(object: *mut sys::zend_object) {
    let handle = ZObj::from_ptr(object).handle();

    DSN_MAP.with(|dsn_map| {
        dsn_map.borrow_mut().remove(&handle);
    });
    DTOR_MAP.with(|dtor_map| {
        if let Some(Some(dtor)) = dtor_map.borrow_mut().remove(&handle) {
            dtor(object);
        }
    });
}

fn after_hook(
    span: Box<dyn Any>, execute_data: &mut ExecuteData, return_value: &mut ZVal,
) -> anyhow::Result<()> {
    if let Some(b) = return_value.as_bool() {
        if !b {
            return after_hook_when_false(
                get_this_mut(execute_data)?,
                &mut span.downcast::<Span>().unwrap(),
            );
        }
    } else if let Some(obj) = return_value.as_mut_z_obj() {
        if obj.get_class().get_name() == &"PDOStatement" {
            return after_hook_when_pdo_statement(get_this_mut(execute_data)?, obj);
        }
    }

    Ok(())
}

fn after_hook_when_false(this: &mut ZObj, span: &mut Span) -> anyhow::Result<()> {
    span.with_span_object_mut(|span| {
        span.is_error = true;
    });

    let info = this.call("errorInfo", [])?;
    let info = info.as_z_arr().context("errorInfo isn't array")?;

    let state = get_error_info_item(info, 0)?.expect_z_str()?.to_str()?;
    let code = &get_error_info_item(info, 1)?.expect_long()?.to_string();
    let error = get_error_info_item(info, 2)?.expect_z_str()?.to_str()?;

    span.with_span_object_mut(|span| {
        span.add_log([("SQLSTATE", state), ("Error Code", code), ("Error", error)]);
    });

    Ok(())
}

fn after_hook_when_pdo_statement(pdo: &mut ZObj, pdo_statement: &mut ZObj) -> anyhow::Result<()> {
    let dsn = DSN_MAP.with(|dsn_map| {
        dsn_map
            .borrow()
            .get(&pdo.handle())
            .cloned()
            .context("DSN not found")
    })?;
    DSN_MAP.with(|dsn_map| {
        dsn_map.borrow_mut().insert(pdo_statement.handle(), dsn);
    });
    hack_dtor(pdo_statement, Some(pdo_statement_dtor));
    Ok(())
}

fn get_error_info_item(info: &ZArr, i: u64) -> anyhow::Result<&ZVal> {
    info.get(i)
        .with_context(|| format!("errorInfo[{}] not exists", i))
}

fn create_exit_span_with_dsn(
    class_name: &str, function_name: &str, dsn: &Dsn,
) -> anyhow::Result<Span> {
    RequestContext::try_with_global_ctx(None, |ctx| {
        let mut span =
            ctx.create_exit_span(&format!("{}->{}", class_name, function_name), &dsn.peer);
        span.with_span_object_mut(|obj| {
            obj.set_span_layer(SpanLayer::Database);
            obj.component_id = COMPONENT_PHP_PDO_ID;
            obj.add_tag("db.type", &dsn.db_type);
            obj.add_tag("db.data_source", &dsn.data_source);
        });
        Ok(span)
    })
}

fn with_dsn<T>(handle: u32, f: impl FnOnce(&Dsn) -> anyhow::Result<T>) -> anyhow::Result<T> {
    DSN_MAP.with(|dsn_map| {
        dsn_map
            .borrow()
            .get(&handle)
            .context("dns not exists")
            .and_then(f)
    })
}

#[derive(Debug, Clone)]
struct Dsn {
    db_type: String,
    data_source: String,
    peer: String,
}

impl FromStr for Dsn {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut ss = s.splitn(2, ":");
        let db_type = ss.next().context("unkonwn db type")?.to_owned();
        let data_source = ss.next().context("unkonwn datasource")?.to_owned();

        let mut host = "unknown";
        let mut port = match &*db_type {
            "mysql" => "3306",
            "oci" => "1521", // Oracle
            "sqlsrv" => "1433",
            "pgsql" => "5432",
            _ => "unknown",
        };

        let ss = data_source.split(";");
        for s in ss {
            let mut kv = s.splitn(2, "=");
            let k = kv.next().context("unkonwn key")?;
            let v = kv.next().context("unkonwn value")?;

            // TODO compact the fields rather than mysql.
            match k {
                "host" => {
                    host = v;
                }
                "port" => {
                    port = v;
                }
                _ => {}
            }
        }

        let peer = format!("{host}:{port}");

        Ok(Dsn {
            db_type,
            data_source,
            peer,
        })
    }
}
