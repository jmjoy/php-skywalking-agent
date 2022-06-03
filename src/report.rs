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
    channel::channel_receive,
    module::{mark_ready_for_request, SERVICE_NAME},
    SKYWALKING_AGENT_SERVER_ADDR, SKYWALKING_AGENT_SERVICE_NAME, SKYWALKING_AGENT_WORKER_THREADS,
};
use anyhow::Context;
use phper::ini::Ini;
use skywalking_rust::{
    common::random_generator::RandomGenerator,
    context::trace_context::TracingContext,
    skywalking_proto::v3::{
        management_service_client::ManagementServiceClient,
        trace_segment_report_service_client::TraceSegmentReportServiceClient, InstanceProperties,
        KeyStringValuePair, SegmentObject,
    },
};
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
    let service_name = SERVICE_NAME.clone();

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

#[tracing::instrument(skip_all)]
async fn start_reporter(server_addr: String, service_name: String) {
    debug!("Starting reporter...");

    let f = async move {
        let endpoint = Endpoint::from_shared(server_addr.clone())?;
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

        info!(server_addr = &*server_addr, "Skywalking server connected");
        mark_ready_for_request();

        let mut report_client = TraceSegmentReportServiceClient::new(channel.clone());
        let mut manage_client = ManagementServiceClient::new(channel);

        let service_instance = RandomGenerator::generate();
        let properties = InstanceProperties {
            service: service_name.clone(),
            service_instance: service_instance.clone(),
            properties: vec![KeyStringValuePair {
                key: "os_name".to_owned(),
                value: "linux".to_owned(),
            }],
            layer: "".to_string(),
        };
        debug!("Report instance properties={:?}", &properties);
        manage_client.report_instance_properties(properties).await?;

        let mut ctx = TracingContext::default(&service_name, &service_instance);
        let span = ctx.create_entry_span("begin").unwrap();
        ctx.finalize_span(span);

        let segment_object = ctx.convert_segment_object();
        let stream = async_stream::stream! { yield segment_object; };
        report_client.collect(stream).await?;

        receive_and_trace().await;

        Ok::<_, anyhow::Error>(())
    };

    if let Err(e) = f.await {
        error!("Start reporter failed: {}", e);
    }
}

#[tracing::instrument(skip_all)]
async fn receive_and_trace() {
    loop {
        let f = async move {
            let data = channel_receive()?;
            warn!("channel receive: {:?}", String::from_utf8(data));
            Ok::<_, anyhow::Error>(())
        };

        if let Err(e) = f.await {
            error!("Start reporter failed: {}", e);
            sleep(Duration::from_secs(10)).await;
        }
    }
}
