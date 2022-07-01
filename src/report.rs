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
    module::{mark_ready_for_request, SERVICE_INSTANCE, SERVICE_NAME},
    util::{current_formatted_time, HOST_NAME, IPS, OS_NAME},
    SKYWALKING_AGENT_SERVER_ADDR, SKYWALKING_AGENT_WORKER_THREADS,
};
use phper::ini::Ini;
use prost::Message;
use skywalking_rust::skywalking_proto::v3::{
    management_service_client::ManagementServiceClient,
    trace_segment_report_service_client::TraceSegmentReportServiceClient, InstanceProperties,
    KeyStringValuePair, SegmentObject,
};
use std::{
    num::NonZeroUsize,
    process,
    thread::{self, available_parallelism},
    time::Duration,
};
use tokio::{
    runtime::{self, Runtime},
    time::sleep,
};
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, error, info, warn};

pub fn init_reporter() {
    let server_addr = Ini::get::<String>(SKYWALKING_AGENT_SERVER_ADDR).unwrap_or_default();

    thread::spawn(move || {
        let rt = new_tokio_runtime();
        rt.block_on(start_reporter(server_addr));
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

async fn start_reporter(server_addr: String) {
    debug!("Starting reporter...");

    let endpoint = match Endpoint::from_shared(server_addr) {
        Ok(endpoint) => endpoint,
        Err(e) => {
            error!("Create endpoint failed: {}", e);
            return;
        }
    };
    let channel = connect(endpoint).await;
    report_instance_properties(channel.clone()).await;
    mark_ready_for_request();
    receive_and_trace(channel).await;
}

#[tracing::instrument(skip_all)]
async fn connect(endpoint: Endpoint) -> Channel {
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

    let uri = &*endpoint.uri().to_string();
    info!(uri, "Skywalking server connected");

    channel
}

#[tracing::instrument(skip_all)]
async fn report_instance_properties(channel: Channel) {
    let mut manage_client = ManagementServiceClient::new(channel);

    loop {
        let properties = vec![
            KeyStringValuePair {
                key: "language".to_owned(),
                value: "php".to_owned(),
            },
            KeyStringValuePair {
                key: "OS Name".to_owned(),
                value: OS_NAME.to_owned(),
            },
            KeyStringValuePair {
                key: "hostname".to_owned(),
                value: HOST_NAME.to_owned(),
            },
            KeyStringValuePair {
                key: "Process No.".to_owned(),
                value: process::id().to_string(),
            },
            KeyStringValuePair {
                key: "ipv4".to_owned(),
                value: IPS.join(","),
            },
            KeyStringValuePair {
                key: "Start Time".to_owned(),
                value: current_formatted_time(),
            },
        ];

        let properties = InstanceProperties {
            service: SERVICE_NAME.clone(),
            service_instance: SERVICE_INSTANCE.clone(),
            properties: properties.clone(),
            layer: "".to_string(),
        };

        match manage_client
            .report_instance_properties(properties.clone())
            .await
        {
            Ok(_) => {
                debug!("Report instance properties, properties: {:?}", properties);
                break;
            }
            Err(e) => {
                warn!("Report instance properties failed, retry after 10s: {}", e);
                sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

#[tracing::instrument(skip_all)]
async fn receive_and_trace(channel: Channel) {
    let mut report_client = TraceSegmentReportServiceClient::new(channel);

    loop {
        let f = async {
            let data = channel_receive()?;
            debug!(length = data.len(), "channel received");

            // TODO Send raw data to avoid encode and decode again.
            let segment: SegmentObject = Message::decode(&*data)?;
            report_client
                .collect(tokio_stream::iter(vec![segment]))
                .await?;

            Ok::<_, anyhow::Error>(())
        };

        if let Err(e) = f.await {
            error!("Receive and trace failed: {}", e);
            sleep(Duration::from_secs(10)).await;
        }
    }
}
