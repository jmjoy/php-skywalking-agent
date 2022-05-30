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
    module::mark_ready_for_request, SKYWALKING_AGENT_SERVER_ADDR, SKYWALKING_AGENT_SERVICE_NAME,
    SKYWALKING_AGENT_WORKER_THREADS,
};
use phper::ini::Ini;
use skywalking_rust::skywalking_proto::v3::trace_segment_report_service_client::TraceSegmentReportServiceClient;
use std::{
    num::NonZeroUsize,
    thread::{self, available_parallelism},
    time::Duration,
};
use tokio::{
    runtime::{self, Runtime},
    time::sleep,
};
use tonic::transport::Endpoint;
use tracing::{debug, error, info, warn};

pub fn init_reporter() {
    let server_addr = Ini::get::<String>(SKYWALKING_AGENT_SERVER_ADDR).unwrap_or_default();
    let service_name = Ini::get::<String>(SKYWALKING_AGENT_SERVICE_NAME).unwrap_or_default();

    thread::spawn(move || {
        let rt = new_tokio_runtime();
        rt.block_on(start_reporter(server_addr, service_name));
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

async fn start_reporter(server_addr: String, _service_name: String) {
    debug!("Starting reporter...");

    let f = async move {
        let endpoint = Endpoint::from_shared(server_addr)?;
        let channel = loop {
            match endpoint.connect().await {
                Ok(channel) => break channel,
                Err(e) => {
                    warn!(
                        "Connect to skywalking server failed, retry after 10s: {}",
                        e
                    );
                    sleep(Duration::from_secs(10)).await;
                }
            }
        };

        info!("Skywalking server connected.");
        mark_ready_for_request();

        let _client = TraceSegmentReportServiceClient::new(channel);

        Ok::<_, anyhow::Error>(())
    };

    if let Err(e) = f.await {
        error!("Start reporter failed: {}", e);
    }
}
