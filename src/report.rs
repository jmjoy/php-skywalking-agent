// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::{SKYWALKING_AGENT_SERVER_ADDR, SKYWALKING_AGENT_WORKER_THREADS};
use phper::ini::Ini;
use skywalking_rust::reporter::grpc::Reporter;
use std::{
    num::NonZeroUsize,
    thread::{self, available_parallelism},
};
use tokio::runtime::{self, Runtime};
use tracing::debug;

pub fn init_reporter() {
    thread::spawn(|| {
        let rt = new_tokio_runtime();
        rt.block_on(start_reporter());
    });
}

fn new_tokio_runtime() -> Runtime {
    let worker_threads = Ini::get::<i64>(SKYWALKING_AGENT_WORKER_THREADS).unwrap_or(0);
    let worker_threads = if worker_threads <= 0 {
        available_parallelism().map(NonZeroUsize::get).unwrap_or(1)
    } else {
        worker_threads as usize
    };

    runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(worker_threads)
        .build()
        .unwrap()
}

async fn start_reporter() {
    debug!("starting reporter");
    let addr = Ini::get::<String>(SKYWALKING_AGENT_SERVER_ADDR).unwrap_or_default();
    let reporter = Reporter::start(addr).await;
}
