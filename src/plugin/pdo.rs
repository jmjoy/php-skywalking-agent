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
use crate::execute::{get_this_mut, validate_num_args, AfterExecuteHook, BeforeExecuteHook, Noop};
use anyhow::Context;
use phper::values::{ExecuteData, ZVal};
use std::{any::Any, borrow::Cow, cell::RefCell, collections::HashMap, str::FromStr};
use tracing::{debug, error};

thread_local! {
    static DSN_MAP: RefCell<HashMap<u32, Dsn>> = Default::default();
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
                Some(self.hook_pdo())
            }
            (Some("PDOStatement"), "execute") => Some(self.hook_pdo_statement_execute()),
            _ => None,
        }
    }
}

impl PdoPlugin {
    fn hook_pdo_construct(&self) -> (Box<BeforeExecuteHook>, Box<AfterExecuteHook>) {
        (
            Box::new(|execute_data| {
                validate_num_args(execute_data, 1)?;

                let handle = get_this_mut(execute_data)?.handle();

                let dsn = execute_data.get_parameter(0);
                let dsn = dsn.as_z_str().context("dsn isn't str")?.to_str()?;
                debug!(dsn, "new PDO");

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

    fn hook_pdo(&self) -> (Box<BeforeExecuteHook>, Box<AfterExecuteHook>) {
        (Noop::noop(), Box::new(after_hook))
    }

    fn hook_pdo_statement_execute(&self) -> (Box<BeforeExecuteHook>, Box<AfterExecuteHook>) {
        (Noop::noop(), Box::new(after_hook))
    }
}

fn after_hook(_: Box<dyn Any>, _: &mut ExecuteData, _: &ZVal) -> anyhow::Result<()> {
    Ok(())
}

#[derive(Debug)]
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
