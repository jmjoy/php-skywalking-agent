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
    channel::{self},
    module::{mark_ready_for_request, SERVICE_INSTANCE, SERVICE_NAME},
    SKYWALKING_AGENT_SERVER_ADDR, SKYWALKING_AGENT_WORKER_THREADS,
};
use libc::{fork, prctl, PR_SET_PDEATHSIG, SIGTERM};
use phper::ini::Ini;
use skywalking::{context::tracer::Tracer, reporter::grpc::GrpcReporter};
use std::{
    future, num::NonZeroUsize, process::exit, thread::available_parallelism, time::Duration,
};
use tokio::{
    runtime::{self, Runtime},
    time::sleep,
};
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, error, info, warn};

pub fn init_worker() {
    let server_addr = Ini::get::<String>(SKYWALKING_AGENT_SERVER_ADDR).unwrap_or_default();
    let worker_threads = worker_threads();
    let service_name = SERVICE_NAME.to_string();
    let service_instance = SERVICE_INSTANCE.to_string();

    unsafe {
        let pid = fork();
        if pid < 0 {
            error!("fork failed");
        } else if pid == 0 {
            prctl(PR_SET_PDEATHSIG, SIGTERM);
            let rt = new_tokio_runtime(worker_threads);
            rt.block_on(start_worker(server_addr, service_name, service_instance));
            exit(0);
        }
    }
}

fn worker_threads() -> usize {
    let worker_threads = Ini::get::<i64>(SKYWALKING_AGENT_WORKER_THREADS).unwrap_or(0);
    if worker_threads <= 0 {
        available_parallelism().map(NonZeroUsize::get).unwrap_or(1)
    } else {
        worker_threads as usize
    }
}

fn new_tokio_runtime(worker_threads: usize) -> Runtime {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(worker_threads)
        .build()
        .unwrap()
}

async fn start_worker(server_addr: String, service_name: String, service_instance: String) {
    debug!("Starting worker...");

    let endpoint = match Endpoint::from_shared(server_addr) {
        Ok(endpoint) => endpoint,
        Err(e) => {
            error!("Create endpoint failed: {}", e);
            return;
        }
    };
    let channel = connect(endpoint).await;

    let tracer = Tracer::new_with_channel(
        service_name,
        service_instance,
        GrpcReporter::new(channel),
        ((), channel::Receiver),
    );

    // report_instance_properties(channel.clone()).await;
    mark_ready_for_request();
    info!("Worker is ready...");

    if let Err(err) = tracer.reporting(future::pending()).await {
        error!(?err, "Tracer reporting failed");
    }

    // let handle = receive_and_trace(channel);
    // handle.await;
}

#[tracing::instrument(skip_all)]
async fn connect(endpoint: Endpoint) -> Channel {
    let channel = loop {
        match endpoint.connect().await {
            Ok(channel) => break channel,
            Err(err) => {
                warn!(?err, "Connect to skywalking server failed, retry after 10s");
                sleep(Duration::from_secs(10)).await;
            }
        }
    };

    let uri = &*endpoint.uri().to_string();
    info!(uri, "Skywalking server connected");

    channel
}

// #[tracing::instrument(skip_all)]
// async fn report_instance_properties(channel: Channel) {
//     let mut manage_client = ManagementServiceClient::new(channel);

//     loop {
//         let properties = vec![
//             KeyStringValuePair {
//                 key: "language".to_owned(),
//                 value: "php".to_owned(),
//             },
//             KeyStringValuePair {
//                 key: "OS Name".to_owned(),
//                 value: OS_NAME.to_owned(),
//             },
//             KeyStringValuePair {
//                 key: "hostname".to_owned(),
//                 value: HOST_NAME.to_owned(),
//             },
//             KeyStringValuePair {
//                 key: "Process No.".to_owned(),
//                 value: process::id().to_string(),
//             },
//             KeyStringValuePair {
//                 key: "ipv4".to_owned(),
//                 value: IPS.join(","),
//             },
//             KeyStringValuePair {
//                 key: "Start Time".to_owned(),
//                 value: current_formatted_time(),
//             },
//         ];

//         let properties = InstanceProperties {
//             service: SERVICE_NAME.clone(),
//             service_instance: SERVICE_INSTANCE.clone(),
//             properties: properties.clone(),
//             layer: "".to_string(),
//         };

//         match manage_client
//             .report_instance_properties(properties.clone())
//             .await
//         {
//             Ok(_) => {
//                 debug!("Report instance properties, properties: {:?}",
// properties);                 break;
//             }
//             Err(e) => {
//                 warn!("Report instance properties failed, retry after 10s:
// {}", e);                 sleep(Duration::from_secs(10)).await;
//             }
//         }
//     }
// }

// #[tracing::instrument(skip_all)]
// async fn receive_and_trace(channel: Channel) {
//     warn!("Start");

//     let mut report_client = TraceSegmentReportServiceClient::new(channel);

//     loop {
//         let f = async {
//             let data = channel_receive()?;

//             warn!(length = data.len(), "Channel received");

//             // TODO Send raw data to avoid encode and decode again.
//             // let segment: SegmentObject = Message::decode(&*data)?;
//             // report_client
//             //     .collect(tokio_stream::iter(vec![segment]))
//             //     .await?;

//             Ok::<_, anyhow::Error>(())
//         };

//         if let Err(e) = f.await {
//             error!("Receive and trace failed: {}", e);
//             sleep(Duration::from_secs(10)).await;
//         }
//     }
// }
